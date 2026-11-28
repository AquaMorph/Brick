//! Safe access to AVFoundation video capture devices, the macOS webcam API
//! (C2).
//!
//! This crate holds all of the unsafe code needed to talk to AVFoundation
//! (R4). The rest of Brick only sees the safe API: [`devices`], [`Device`],
//! and [`usb::UsbDevice`] for the webcam controls AVFoundation does not offer.
//!
//! Almost every AVFoundation call is `unsafe` in the Rust bindings only
//! because the bindings cannot check Objective-C's rules for themselves. The
//! invariants the calls here rely on are:
//!
//! - Every object passed to a method has the class the method expects, which
//!   the bindings' types guarantee.
//! - Methods are only called with the arguments their documentation allows,
//!   for example `AVMediaTypeVideo` as the media type.
//! - The device is locked for configuration (`lockForConfiguration`) before
//!   any of its settings are changed, as AVFoundation requires.

#![cfg(target_os = "macos")]

pub mod usb;

use std::io;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceFormat,
    AVCaptureDeviceInput, AVCaptureExposureMode, AVCaptureFocusMode, AVCaptureOutput,
    AVCaptureSession, AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate,
    AVCaptureWhiteBalanceMode, AVMediaTypeVideo,
};
use objc2_core_media::{CMSampleBuffer, CMVideoFormatDescriptionGetDimensions};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_32BGRA, kCVReturnSuccess,
};
use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};

/// A video capture device found by [`devices`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Stays the same across connections, restarts, and reboots.
    pub unique_id: String,
    /// The device's name, such as "FaceTime HD Camera".
    pub name: String,
}

/// The video capture devices currently connected.
pub fn devices() -> Vec<DeviceInfo> {
    video_devices()
        .iter()
        .map(|device| DeviceInfo {
            // SAFETY: plain property reads on a valid device.
            unique_id: unsafe { device.uniqueID() }.to_string(),
            // SAFETY: as above.
            name: unsafe { device.localizedName() }.to_string(),
        })
        .collect()
}

fn video_devices() -> Vec<Retained<AVCaptureDevice>> {
    let Some(video) = media_type_video() else {
        return Vec::new();
    };
    // The discovery session that replaces this needs a device type for
    // external webcams that only exists from macOS 14, and linking to it
    // would stop Brick from starting on older systems.
    // SAFETY: `video` is a valid media type.
    #[allow(deprecated)]
    let devices = unsafe { AVCaptureDevice::devicesWithMediaType(video) };
    devices.to_vec()
}

fn media_type_video() -> Option<&'static NSString> {
    // SAFETY: reading an immutable framework constant.
    unsafe { AVMediaTypeVideo }
}

/// Asks the user for permission to use the camera, the first time only, and
/// waits for the answer. macOS shows black frames until permission is given.
fn authorize() -> io::Result<()> {
    let video = media_type_video().ok_or_else(|| other("AVFoundation has no video media type"))?;
    // SAFETY: `video` is AVMediaTypeVideo, which this method accepts.
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(video) };
    let granted = match status {
        AVAuthorizationStatus::Authorized => true,
        AVAuthorizationStatus::NotDetermined => {
            let (sender, receiver) = mpsc::channel();
            let handler = RcBlock::new(move |granted: Bool| {
                let _ = sender.send(granted.as_bool());
            });
            // SAFETY: `video` is AVMediaTypeVideo, and the block may be called
            // on any thread, which `Sender` allows.
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(video, &handler)
            };
            receiver.recv().unwrap_or(false)
        }
        _ => false,
    };
    if granted {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Brick is not allowed to use the camera. Allow it in System Settings > \
             Privacy & Security > Camera",
        ))
    }
}

/// One video frame, 8 bit RGBA.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// An open video capture device.
pub struct Device {
    device: Retained<AVCaptureDevice>,
    session: Retained<AVCaptureSession>,
    output: Retained<AVCaptureVideoDataOutput>,
    /// Kept alive while it is the output's delegate. Fields are dropped
    /// after `Drop::drop` has removed it.
    _receiver: Retained<FrameReceiver>,
    frames: Arc<Frames>,
    /// Keeps the queue frames are delivered on alive while the session runs.
    _queue: DispatchRetained<DispatchQueue>,
}

// SAFETY: AVCaptureDevice and AVCaptureSession may be used from any thread,
// one at a time; Apple recommends starting sessions off the main thread.
// `Device` is not `Sync`, so it is only ever used from one thread at a time.
// The frame receiver is only touched by AVFoundation on its own queue, and the
// state it shares with `Device` is behind a mutex.
unsafe impl Send for Device {}

impl Device {
    /// Opens the device with `unique_id`, asking for camera permission first
    /// if the user has not been asked yet.
    pub fn open(unique_id: &str) -> io::Result<Device> {
        authorize()?;
        let device = video_devices()
            .into_iter()
            // SAFETY: plain property read on a valid device.
            .find(|d| unsafe { d.uniqueID() }.to_string() == unique_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "not connected"))?;

        // SAFETY: `device` is a valid capture device.
        let input = unsafe {
            AVCaptureDeviceInput::initWithDevice_error(AVCaptureDeviceInput::alloc(), &device)
        }
        .map_err(|e| other(e.localizedDescription()))?;

        let frames = Arc::new(Frames::default());
        let receiver = FrameReceiver::new(Arc::clone(&frames));
        let queue = DispatchQueue::new("brick.camera.frames", None);

        // SAFETY: creating empty capture objects has no requirements.
        let session = unsafe { AVCaptureSession::new() };
        // SAFETY: as above.
        let output = unsafe { AVCaptureVideoDataOutput::new() };
        // SAFETY: the settings dictionary uses a documented key with an
        // NSNumber value. The delegate implements the sample buffer protocol
        // and the queue is serial, as AVFoundation requires; both are kept
        // alive by `Device` for as long as the output is.
        unsafe {
            let format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
            let key: &NSString = kCVPixelBufferPixelFormatTypeKey.as_ref();
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[&*format]);
            output.setVideoSettings(Some(&settings));
            output.setAlwaysDiscardsLateVideoFrames(true);
            output.setSampleBufferDelegate_queue(
                Some(ProtocolObject::from_ref(&*receiver)),
                Some(&queue),
            );
        }

        // SAFETY: inputs and outputs are only added after checking the
        // session accepts them, inside a configuration block.
        unsafe {
            session.beginConfiguration();
            let added = session.canAddInput(&input) && {
                session.addInput(&input);
                session.canAddOutput(&output)
            };
            if added {
                session.addOutput(&output);
            }
            session.commitConfiguration();
            if !added {
                return Err(other("the camera cannot be used for video capture"));
            }
        }

        let device = Device {
            device,
            session,
            output,
            _receiver: receiver,
            frames,
            _queue: queue,
        };
        // The session picks a preset when the input is added. Choosing the
        // native size afterwards overrides it.
        if let Some(&(width, height)) = device.sizes().first() {
            device.set_size(width, height)?;
        }
        Ok(device)
    }

    /// Whether the device is still plugged in.
    pub fn is_connected(&self) -> bool {
        // SAFETY: plain property read on a valid device.
        unsafe { self.device.isConnected() }
    }

    /// The frame sizes the device offers, largest first.
    pub fn sizes(&self) -> Vec<(u32, u32)> {
        let mut sizes: Vec<(u32, u32)> = self.formats().iter().map(|f| dimensions(f)).collect();
        sizes.sort_by_key(|(w, h)| std::cmp::Reverse(u64::from(*w) * u64::from(*h)));
        sizes.dedup();
        sizes
    }

    /// The size of the frames the device sends.
    pub fn size(&self) -> (u32, u32) {
        // SAFETY: plain property read on a valid device.
        let format = unsafe { self.device.activeFormat() };
        dimensions(&format)
    }

    /// Changes the frame size. Of the device's formats with that size, the
    /// one with the highest frame rate is used, so live view stays smooth.
    pub fn set_size(&self, width: u32, height: u32) -> io::Result<()> {
        let format = self
            .formats()
            .into_iter()
            .filter(|f| dimensions(f) == (width, height))
            .max_by(|a, b| max_frame_rate(a).total_cmp(&max_frame_rate(b)))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::Unsupported, "the size is not supported")
            })?;
        self.configure(|device| {
            // SAFETY: `format` is one of the device's own formats, and
            // `configure` holds the configuration lock.
            unsafe { device.setActiveFormat(&format) }
        })
    }

    /// Fixes exposure, white balance, and focus where the device allows it,
    /// so the picture does not change between frames (C8). Returns whether
    /// the exposure could be fixed.
    pub fn lock_automatic(&self) -> io::Result<bool> {
        self.configure(|device| {
            // SAFETY: each mode is only set after the device says it supports
            // it, and `configure` holds the configuration lock.
            unsafe {
                let exposure = device.isExposureModeSupported(AVCaptureExposureMode::Locked);
                if exposure {
                    device.setExposureMode(AVCaptureExposureMode::Locked);
                }
                if device.isWhiteBalanceModeSupported(AVCaptureWhiteBalanceMode::Locked) {
                    device.setWhiteBalanceMode(AVCaptureWhiteBalanceMode::Locked);
                }
                if device.isFocusModeSupported(AVCaptureFocusMode::Locked) {
                    device.setFocusMode(AVCaptureFocusMode::Locked);
                }
                exposure
            }
        })
    }

    /// Starts or stops sending frames. Stopping turns off the camera's
    /// light.
    pub fn set_running(&mut self, on: bool) -> io::Result<()> {
        if on && !self.is_connected() {
            return Err(disconnected());
        }
        // SAFETY: the session was fully configured in `open`. Both calls
        // block until the session has started or stopped.
        unsafe {
            if on {
                self.session.startRunning();
            } else {
                self.session.stopRunning();
            }
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        // SAFETY: plain property read on a valid session.
        unsafe { self.session.isRunning() }
    }

    /// Waits for the newest frame that has not been returned yet. Frames
    /// that arrived in the meantime are skipped, so live view never lags.
    pub fn latest_frame(&mut self, timeout: Duration) -> io::Result<Image> {
        let after = self.frames.taken();
        self.wait(after, timeout)
    }

    /// Waits for a frame the device sends after this call. Frames that were
    /// already waiting are skipped.
    pub fn next_frame(&mut self, timeout: Duration) -> io::Result<Image> {
        let after = self.frames.newest();
        self.wait(after, timeout)
    }

    fn wait(&mut self, after: u64, timeout: Duration) -> io::Result<Image> {
        if !self.is_running() {
            return Err(other("the camera is not running"));
        }
        match self.frames.wait(after, timeout) {
            Some(frame) => Ok(frame),
            None if !self.is_connected() => Err(disconnected()),
            None => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the camera sent no frame",
            )),
        }
    }

    fn formats(&self) -> Vec<Retained<AVCaptureDeviceFormat>> {
        // SAFETY: plain property read on a valid device.
        unsafe { self.device.formats() }.to_vec()
    }

    fn configure<T>(&self, change: impl FnOnce(&AVCaptureDevice) -> T) -> io::Result<T> {
        // SAFETY: the lock is released below on every path.
        unsafe { self.device.lockForConfiguration() }.map_err(|e| {
            other(format!(
                "could not configure the camera: {}",
                e.localizedDescription()
            ))
        })?;
        let result = change(&self.device);
        // SAFETY: the lock was taken above.
        unsafe { self.device.unlockForConfiguration() };
        Ok(result)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: removing the delegate before the receiver is released
        // means AVFoundation never calls into a freed object.
        unsafe {
            self.session.stopRunning();
            self.output.setSampleBufferDelegate_queue(None, None);
        }
    }
}

/// The newest frame from the device, shared between AVFoundation's queue and
/// the thread reading frames.
#[derive(Default)]
struct Frames {
    state: Mutex<FrameState>,
    arrived: Condvar,
}

#[derive(Default)]
struct FrameState {
    /// Counts the frames received.
    sequence: u64,
    /// The sequence number of the last frame returned.
    taken: u64,
    newest: Option<Bgra>,
}

struct Bgra {
    width: u32,
    height: u32,
    /// Rows of `width * 4` bytes, without padding.
    data: Vec<u8>,
}

impl Frames {
    fn put(&self, frame: Bgra) {
        if let Ok(mut state) = self.state.lock() {
            state.sequence += 1;
            state.newest = Some(frame);
            self.arrived.notify_all();
        }
    }

    fn newest(&self) -> u64 {
        self.state.lock().map(|s| s.sequence).unwrap_or(0)
    }

    fn taken(&self) -> u64 {
        self.state.lock().map(|s| s.taken).unwrap_or(0)
    }

    /// Waits for a frame with a sequence number above `after`.
    fn wait(&self, after: u64, timeout: Duration) -> Option<Image> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().ok()?;
        while state.sequence <= after || state.newest.is_none() {
            let left = deadline.checked_duration_since(Instant::now())?;
            state = self.arrived.wait_timeout(state, left).ok()?.0;
        }
        state.taken = state.sequence;
        let frame = state.newest.take()?;
        drop(state);
        Some(frame.into_rgba())
    }
}

impl Bgra {
    fn into_rgba(mut self) -> Image {
        for pixel in self.data.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        Image {
            width: self.width,
            height: self.height,
            rgba: self.data,
        }
    }
}

define_class!(
    /// Receives frames from AVFoundation on its queue.
    // SAFETY: NSObject has no subclassing requirements, and the class does
    // not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "BrickFrameReceiver"]
    #[ivars = Arc<Frames>]
    struct FrameReceiver;

    // SAFETY: NSObjectProtocol has no extra requirements.
    unsafe impl NSObjectProtocol for FrameReceiver {}

    // SAFETY: the method below has the signature the protocol declares.
    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for FrameReceiver {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn did_output(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            if let Some(frame) = copy_frame(sample) {
                self.ivars().put(frame);
            }
        }
    }
);

impl FrameReceiver {
    fn new(frames: Arc<Frames>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(frames);
        // SAFETY: NSObject's `init` is the designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// Copies a sample buffer's pixels out, so AVFoundation can reuse its buffer
/// right away.
fn copy_frame(sample: &CMSampleBuffer) -> Option<Bgra> {
    // SAFETY: `sample` is the valid buffer AVFoundation passed in.
    let pixels = unsafe { sample.image_buffer() }?;
    if CVPixelBufferGetPixelFormatType(&pixels) != kCVPixelFormatType_32BGRA {
        return None;
    }
    // SAFETY: the buffer is unlocked again below before it is released.
    let locked = unsafe { CVPixelBufferLockBaseAddress(&pixels, CVPixelBufferLockFlags::ReadOnly) };
    if locked != kCVReturnSuccess {
        return None;
    }
    let width = CVPixelBufferGetWidth(&pixels);
    let height = CVPixelBufferGetHeight(&pixels);
    let stride = CVPixelBufferGetBytesPerRow(&pixels);
    let base = CVPixelBufferGetBaseAddress(&pixels).cast::<u8>();
    let row = width * 4;
    let mut data = Vec::new();
    if !base.is_null() && stride >= row {
        data.reserve_exact(row * height);
        for y in 0..height {
            // SAFETY: the base address is locked, and a 32BGRA buffer holds
            // `height` rows of `stride` bytes, each starting with `width`
            // four byte pixels.
            let line = unsafe { std::slice::from_raw_parts(base.add(y * stride), row) };
            data.extend_from_slice(line);
        }
    }
    // SAFETY: the buffer was locked with the same flags above.
    unsafe { CVPixelBufferUnlockBaseAddress(&pixels, CVPixelBufferLockFlags::ReadOnly) };
    if data.is_empty() {
        return None;
    }
    Some(Bgra {
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        data,
    })
}

fn dimensions(format: &AVCaptureDeviceFormat) -> (u32, u32) {
    // SAFETY: a capture device format's description is a video format
    // description.
    let size = unsafe { CMVideoFormatDescriptionGetDimensions(&format.formatDescription()) };
    (size.width.max(0) as u32, size.height.max(0) as u32)
}

fn max_frame_rate(format: &AVCaptureDeviceFormat) -> f64 {
    // SAFETY: plain property reads on a valid format and its ranges.
    unsafe { format.videoSupportedFrameRateRanges() }
        .to_vec()
        .into_iter()
        // SAFETY: as above.
        .map(|range| unsafe { range.maxFrameRate() })
        .fold(0.0, f64::max)
}

fn disconnected() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "the camera was disconnected")
}

fn other(message: impl ToString) -> io::Error {
    io::Error::other(message.to_string())
}

/// Whether an error means the device is gone.
pub fn is_disconnected(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotConnected
}

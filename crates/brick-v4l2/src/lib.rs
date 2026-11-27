//! Safe access to V4L2 video capture devices, the Linux webcam API (C2).
//!
//! This crate holds all of the unsafe code needed to talk to V4L2 (R4). The
//! rest of Brick only sees the safe API: [`devices`], [`Device`], and
//! [`Stream`].
//!
//! The soundness of every `ioctl` here rests on one invariant: each request
//! code is built from the size of the Rust struct passed with it, and each of
//! those structs in [`sys`] has exactly the layout of the kernel's struct for
//! that request. The layouts are checked against the kernel's sizes at compile
//! time on 64 bit targets.

#![cfg(target_os = "linux")]

mod sys;

use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use sys::{
    CID_AUTO_WHITE_BALANCE, CID_EXPOSURE_ABSOLUTE, CID_EXPOSURE_AUTO, CID_EXPOSURE_AUTO_PRIORITY,
    CID_FOCUS_ABSOLUTE, CID_FOCUS_AUTO, CID_GAIN, CID_WHITE_BALANCE_TEMPERATURE, EXPOSURE_MANUAL,
};

/// A four character pixel format code such as `MJPG` or `YUYV`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCc(pub [u8; 4]);

impl FourCc {
    pub const MJPG: FourCc = FourCc(*b"MJPG");
    pub const YUYV: FourCc = FourCc(*b"YUYV");

    fn from_u32(code: u32) -> Self {
        FourCc(code.to_le_bytes())
    }

    fn to_u32(self) -> u32 {
        u32::from_le_bytes(self.0)
    }
}

impl std::fmt::Display for FourCc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

/// A video capture device found by [`devices`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub path: PathBuf,
    /// The device's name, such as "Laptop Camera".
    pub card: String,
    /// Where the device is connected. Stays the same while it stays plugged
    /// into the same port, even if its `/dev/video` number changes.
    pub bus_info: String,
}

/// The video capture devices that can stream, sorted by path. Devices that
/// cannot be opened are skipped.
pub fn devices() -> Vec<DeviceInfo> {
    let Ok(entries) = fs::read_dir("/dev") else {
        return Vec::new();
    };
    let mut found: Vec<DeviceInfo> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("video"))
        .filter_map(|e| Device::open(&e.path()).ok()?.info().ok())
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// The size and layout of the frames a device sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixFormat {
    pub width: u32,
    pub height: u32,
    pub fourcc: FourCc,
    pub bytes_per_line: u32,
}

/// The range of an integer, boolean, or menu control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlInfo {
    pub id: u32,
    pub minimum: i32,
    pub maximum: i32,
    pub step: i32,
    pub default: i32,
    /// The control currently has no effect, for example a manual exposure
    /// time while auto exposure is on.
    pub inactive: bool,
}

/// An open video capture device.
pub struct Device {
    path: PathBuf,
    file: File,
}

impl Device {
    pub fn open(path: &Path) -> io::Result<Device> {
        let file = File::options().read(true).write(true).open(path)?;
        Ok(Device {
            path: path.to_owned(),
            file,
        })
    }

    /// The device's name and connection, if it can capture and stream video.
    pub fn info(&self) -> io::Result<DeviceInfo> {
        let mut cap = sys::Capability::default();
        sys::ioctl(&self.file, sys::QUERYCAP, &mut cap)?;
        let caps = if cap.capabilities & sys::CAP_DEVICE_CAPS != 0 {
            cap.device_caps
        } else {
            cap.capabilities
        };
        let wanted = sys::CAP_VIDEO_CAPTURE | sys::CAP_STREAMING;
        if caps & wanted != wanted {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "not a streaming video capture device",
            ));
        }
        Ok(DeviceInfo {
            path: self.path.clone(),
            card: c_string(&cap.card),
            bus_info: c_string(&cap.bus_info),
        })
    }

    /// The pixel formats the device can send.
    pub fn formats(&self) -> io::Result<Vec<FourCc>> {
        let mut formats = Vec::new();
        for index in 0.. {
            let mut desc = sys::FmtDesc {
                index,
                type_: sys::BUF_TYPE_VIDEO_CAPTURE,
                ..Default::default()
            };
            match sys::ioctl(&self.file, sys::ENUM_FMT, &mut desc) {
                Ok(()) => formats.push(FourCc::from_u32(desc.pixelformat)),
                Err(e) if e.raw_os_error() == Some(libc::EINVAL) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(formats)
    }

    /// The frame sizes the device offers for a format. A device with a
    /// continuous range reports only its largest size.
    pub fn frame_sizes(&self, fourcc: FourCc) -> io::Result<Vec<(u32, u32)>> {
        let mut sizes = Vec::new();
        for index in 0.. {
            let mut size = sys::FrmSizeEnum {
                index,
                pixel_format: fourcc.to_u32(),
                ..Default::default()
            };
            match sys::ioctl(&self.file, sys::ENUM_FRAMESIZES, &mut size) {
                Ok(()) if size.type_ == sys::FRMSIZE_TYPE_DISCRETE => {
                    sizes.push((size.data[0], size.data[1]));
                }
                Ok(()) => {
                    // Stepwise or continuous: data is min_w, max_w, step_w,
                    // min_h, max_h, step_h.
                    sizes.push((size.data[1], size.data[4]));
                    break;
                }
                Err(e) if e.raw_os_error() == Some(libc::EINVAL) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(sizes)
    }

    /// Asks for a format and size. The device may pick the closest it
    /// supports, which is returned.
    pub fn set_format(&self, fourcc: FourCc, width: u32, height: u32) -> io::Result<PixFormat> {
        let mut format = sys::Format {
            type_: sys::BUF_TYPE_VIDEO_CAPTURE,
            ..Default::default()
        };
        format.fmt.pix.width = width;
        format.fmt.pix.height = height;
        format.fmt.pix.pixelformat = fourcc.to_u32();
        format.fmt.pix.field = sys::FIELD_NONE;
        sys::ioctl(&self.file, sys::S_FMT, &mut format)?;
        let pix = format.fmt.pix;
        Ok(PixFormat {
            width: pix.width,
            height: pix.height,
            fourcc: FourCc::from_u32(pix.pixelformat),
            bytes_per_line: pix.bytesperline,
        })
    }

    /// The range of a control, or `None` if the device does not have it or
    /// has it disabled.
    pub fn query_control(&self, id: u32) -> io::Result<Option<ControlInfo>> {
        let mut query = sys::QueryCtrl {
            id,
            ..Default::default()
        };
        match sys::ioctl(&self.file, sys::QUERYCTRL, &mut query) {
            Ok(()) if query.flags & sys::CTRL_FLAG_DISABLED != 0 => Ok(None),
            Ok(()) => Ok(Some(ControlInfo {
                id,
                minimum: query.minimum,
                maximum: query.maximum,
                step: query.step.max(1),
                default: query.default_value,
                inactive: query.flags & sys::CTRL_FLAG_INACTIVE != 0,
            })),
            Err(e) if e.raw_os_error() == Some(libc::EINVAL) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn control(&self, id: u32) -> io::Result<i32> {
        let mut control = sys::Control { id, value: 0 };
        sys::ioctl(&self.file, sys::G_CTRL, &mut control)?;
        Ok(control.value)
    }

    pub fn set_control(&self, id: u32, value: i32) -> io::Result<()> {
        let mut control = sys::Control { id, value };
        sys::ioctl(&self.file, sys::S_CTRL, &mut control)
    }

    /// Starts streaming frames into `buffers` memory mapped buffers.
    pub fn stream(&self, buffers: u32) -> io::Result<Stream> {
        Stream::start(self.file.try_clone()?, buffers)
    }
}

/// A buffer shared with the driver through `mmap`.
struct Mapping {
    ptr: *mut libc::c_void,
    len: usize,
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: `ptr` and `len` are exactly what a successful `mmap`
        // returned and nothing else unmaps them. No slices into the mapping
        // outlive `&self` borrows, so none exist once it is dropped.
        unsafe {
            libc::munmap(self.ptr, self.len);
        }
    }
}

/// A running video stream. Dropping it stops the stream.
pub struct Stream {
    file: File,
    buffers: Vec<Mapping>,
}

// SAFETY: the mappings are plain memory owned by this stream. They are only
// read through `&mut self`, so moving the stream to another thread cannot
// race with another user of the same pointers.
unsafe impl Send for Stream {}

impl Stream {
    fn start(file: File, count: u32) -> io::Result<Stream> {
        let mut request = sys::RequestBuffers {
            count,
            type_: sys::BUF_TYPE_VIDEO_CAPTURE,
            memory: sys::MEMORY_MMAP,
            ..Default::default()
        };
        sys::ioctl(&file, sys::REQBUFS, &mut request)?;
        let mut stream = Stream {
            file,
            buffers: Vec::new(),
        };
        for index in 0..request.count {
            let mut buffer = sys::Buffer::new(index);
            sys::ioctl(&stream.file, sys::QUERYBUF, &mut buffer)?;
            let len = buffer.length as usize;
            // QUERYBUF fills in the offset for memory mapped buffers.
            let offset = buffer.m.offset;
            // SAFETY: mapping a new region chosen by the kernel (null hint)
            // cannot alias any existing Rust memory. The length and offset
            // are the ones the driver gave for this buffer.
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    stream.file.as_raw_fd(),
                    libc::off_t::from(offset),
                )
            };
            if ptr == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            stream.buffers.push(Mapping { ptr, len });
            sys::ioctl(&stream.file, sys::QBUF, &mut buffer)?;
        }
        let mut kind = sys::BUF_TYPE_VIDEO_CAPTURE as libc::c_int;
        sys::ioctl(&stream.file, sys::STREAMON, &mut kind)?;
        Ok(stream)
    }

    /// Whether a frame is waiting to be read.
    pub fn ready(&self) -> io::Result<bool> {
        self.poll(Duration::ZERO)
    }

    /// Waits up to `timeout` for the next frame and copies it out. Frames
    /// the driver marks as damaged, which some webcams send right after a
    /// setting changes, are skipped. Returns an error of kind `TimedOut` if
    /// no good frame came, and `ENODEV` once the device is unplugged.
    pub fn next_frame(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if !self.poll(left)? {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the camera sent no frame",
                ));
            }
            if let Some(frame) = self.dequeue()? {
                return Ok(frame);
            }
        }
    }

    /// Waits up to `timeout` for a frame, then returns the newest one ready,
    /// dropping older frames still queued. Live view uses this so it never
    /// falls behind the camera.
    pub fn latest_frame(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        let mut frame = self.next_frame(timeout)?;
        while self.ready()? {
            if let Some(newer) = self.dequeue()? {
                frame = newer;
            }
        }
        Ok(frame)
    }

    /// Takes the next filled buffer, copies it out, and hands the buffer back
    /// to the driver. `None` if the frame was damaged.
    fn dequeue(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut buffer = sys::Buffer::new(0);
        sys::ioctl(&self.file, sys::DQBUF, &mut buffer)?;
        let mapping = self
            .buffers
            .get(buffer.index as usize)
            .ok_or_else(|| io::Error::other("the driver returned an unknown buffer"))?;
        let used = (buffer.bytesused as usize).min(mapping.len);
        let damaged = buffer.flags & sys::BUF_FLAG_ERROR != 0;
        let data = if damaged {
            None
        } else {
            // SAFETY: the mapping is `mapping.len` bytes of readable memory
            // that lives as long as `self`, and `used` is at most that. The
            // driver has handed this buffer back (DQBUF) and does not write
            // to it until it is queued again below, after the copy.
            Some(unsafe { std::slice::from_raw_parts(mapping.ptr.cast::<u8>(), used) }.to_vec())
        };
        sys::ioctl(&self.file, sys::QBUF, &mut buffer)?;
        Ok(data)
    }

    fn poll(&self, timeout: Duration) -> io::Result<bool> {
        let mut fd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
        loop {
            // SAFETY: `fd` is a valid pollfd and the count of 1 matches it.
            let result = unsafe { libc::poll(&mut fd, 1, millis) };
            if result >= 0 {
                break;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        if fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(io::Error::from_raw_os_error(libc::ENODEV));
        }
        Ok(fd.revents & libc::POLLIN != 0)
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let mut kind = sys::BUF_TYPE_VIDEO_CAPTURE as libc::c_int;
        let _ = sys::ioctl(&self.file, sys::STREAMOFF, &mut kind);
        // Buffers must be unmapped before they are released.
        self.buffers.clear();
        let mut request = sys::RequestBuffers {
            count: 0,
            type_: sys::BUF_TYPE_VIDEO_CAPTURE,
            memory: sys::MEMORY_MMAP,
            ..Default::default()
        };
        let _ = sys::ioctl(&self.file, sys::REQBUFS, &mut request);
    }
}

/// Converts a packed YUYV 4:2:2 frame (BT.601) to 8 bit RGB. `None` if
/// `data` is too short for the format.
pub fn yuyv_to_rgb(data: &[u8], format: PixFormat) -> Option<Vec<u8>> {
    let (width, height) = (format.width as usize, format.height as usize);
    let stride = (format.bytes_per_line as usize).max(width * 2);
    if height == 0 || data.len() < stride * (height - 1) + width * 2 {
        return None;
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in data.chunks(stride).take(height) {
        for pair in row[..width * 2].chunks_exact(4) {
            let (y0, u, y1, v) = (pair[0], pair[1], pair[2], pair[3]);
            rgb.extend_from_slice(&yuv(y0, u, v));
            rgb.extend_from_slice(&yuv(y1, u, v));
        }
    }
    Some(rgb)
}

fn yuv(y: u8, u: u8, v: u8) -> [u8; 3] {
    let c = (f32::from(y) - 16.0) * 1.164;
    let d = f32::from(u) - 128.0;
    let e = f32::from(v) - 128.0;
    let clamp = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    [
        clamp(c + 1.596 * e),
        clamp(c - 0.392 * d - 0.813 * e),
        clamp(c + 2.017 * d),
    ]
}

/// Whether an error means the device is gone.
pub fn is_disconnected(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::ENODEV | libc::ENXIO | libc::ENOENT)
    )
}

fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_matches_the_kernel_encoding() {
        // V4L2_PIX_FMT_MJPEG is v4l2_fourcc('M', 'J', 'P', 'G').
        assert_eq!(FourCc::MJPG.to_u32(), 0x4750_4a4d);
        assert_eq!(FourCc::from_u32(0x5659_5559), FourCc::YUYV);
    }

    #[test]
    fn yuyv_grey_stays_grey() {
        let format = PixFormat {
            width: 2,
            height: 1,
            fourcc: FourCc::YUYV,
            bytes_per_line: 4,
        };
        assert_eq!(
            yuyv_to_rgb(&[126, 128, 126, 128], format),
            Some(vec![128; 6])
        );
        assert_eq!(yuyv_to_rgb(&[0; 3], format), None);
    }

    #[test]
    fn c_strings_stop_at_nul() {
        assert_eq!(c_string(b"Laptop Camera\0\0garbage"), "Laptop Camera");
        assert_eq!(c_string(b"full"), "full");
    }

    #[test]
    fn listing_devices_does_not_fail() {
        for device in devices() {
            assert!(!device.card.is_empty());
        }
    }
}

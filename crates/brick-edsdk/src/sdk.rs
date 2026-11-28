//! The thread that makes every EDSDK call.
//!
//! On macOS the SDK finds cameras through ImageCaptureCore, which reports
//! them on the main thread's run loop. The SDK thread only sees cameras while
//! the main thread runs its run loop, as the program's window event loop
//! does.
//!
//! The invariants the unsafe code here relies on are:
//!
//! - Every SDK function is called with arguments of the types and sizes its
//!   header declares ([`sys`]), and property values are read and written
//!   through buffers of exactly the size passed with them.
//! - Every reference the SDK hands out is owned by exactly one [`Ref`], which
//!   releases it once. References are only used while their `Ref` is alive.
//! - SDK calls are only made on the SDK thread, after `EdsInitializeSDK`
//!   succeeded, and camera calls only while the camera's session is open.
//! - The context pointer given to the event handlers points to the camera's
//!   boxed [`Events`], which stays at the same address and alive until the
//!   handlers are removed again when the camera is closed.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::sys::{self, EdsBaseRef, EdsError};
use crate::{DeviceInfo, Error, File, Result};

/// How often camera events are collected while nothing else is happening.
const EVENT_INTERVAL: Duration = Duration::from_millis(50);
/// How often camera events are collected while waiting for a capture.
const CAPTURE_POLL: Duration = Duration::from_millis(10);
/// How long a busy camera is given before a call fails.
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);
const BUSY_RETRY: Duration = Duration::from_millis(50);

type Job = Box<dyn FnOnce(&mut State) + Send>;

static SDK: OnceLock<std::result::Result<Sender<Job>, Error>> = OnceLock::new();

/// Runs `job` on the SDK thread, starting the thread first if needed, and
/// waits for its result.
fn call<T: Send + 'static>(
    job: impl FnOnce(&mut State) -> Result<T> + Send + 'static,
) -> Result<T> {
    let sender = SDK.get_or_init(start).as_ref().map_err(Clone::clone)?;
    let (reply, answer) = mpsc::channel();
    sender
        .send(Box::new(move |state| {
            let _ = reply.send(job(state));
        }))
        .map_err(|_| Error::Stopped("the SDK thread ended".into()))?;
    answer
        .recv()
        .map_err(|_| Error::Stopped("the SDK thread ended".into()))?
}

fn start() -> std::result::Result<Sender<Job>, Error> {
    let (jobs, receiver) = mpsc::channel::<Job>();
    let (ready, started) = mpsc::channel();
    thread::Builder::new()
        .name("edsdk".into())
        .spawn(move || {
            let Some(_sdk) = Initialized::new(&ready) else {
                return;
            };
            let mut state = State::default();
            loop {
                match receiver.recv_timeout(EVENT_INTERVAL) {
                    Ok(job) => job(&mut state),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                state.handle_events();
            }
            // Sessions are closed before the SDK is terminated.
            drop(state);
        })
        .map_err(|e| Error::Stopped(e.to_string()))?;
    started
        .recv()
        .map_err(|_| Error::Stopped("the SDK thread ended".into()))??;
    Ok(jobs)
}

/// The SDK, initialized on the SDK thread. Dropping it terminates the SDK.
struct Initialized;

impl Initialized {
    /// Reports through `ready` whether the SDK started.
    fn new(ready: &Sender<Result<()>>) -> Option<Self> {
        #[cfg(target_os = "windows")]
        // SAFETY: Called once at the start of the SDK thread, which is the
        // only thread that makes SDK calls. A null reserved pointer is
        // required.
        unsafe {
            sys::CoInitializeEx(std::ptr::null_mut(), sys::COINIT_APARTMENTTHREADED);
        }
        // SAFETY: Called once, on the SDK thread, before any other SDK call.
        let result = check(unsafe { sys::EdsInitializeSDK() });
        let started = result.is_ok();
        let _ = ready.send(result);
        started.then_some(Self)
    }
}

impl Drop for Initialized {
    fn drop(&mut self) {
        // SAFETY: The SDK was initialized on this thread and every reference
        // to its objects has been released, since the `State` holding them
        // is dropped first.
        unsafe {
            sys::EdsTerminateSDK();
        }
        #[cfg(target_os = "windows")]
        // SAFETY: Balances the `CoInitializeEx` made when the SDK started, on
        // the same thread.
        unsafe {
            sys::CoUninitialize();
        }
    }
}

fn check(error: EdsError) -> Result<()> {
    match error {
        sys::ERR_OK => Ok(()),
        sys::ERR_COMM_DISCONNECTED
        | sys::ERR_DEVICE_NOT_FOUND
        | sys::ERR_DEVICE_INVALID
        | sys::ERR_SESSION_NOT_OPEN
        | sys::ERR_COMM_USB_BUS_ERR => Err(Error::Disconnected),
        code => Err(Error::Sdk(code)),
    }
}

/// Repeats `call` while the camera reports that it is busy, collecting
/// events in between so the camera can finish what it is doing.
fn retry_busy(mut call: impl FnMut() -> EdsError) -> Result<()> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        let error = call();
        if error != sys::ERR_DEVICE_BUSY || Instant::now() >= deadline {
            return check(error);
        }
        thread::sleep(BUSY_RETRY);
        get_event();
    }
}

/// Lets the SDK call the event handlers for whatever happened.
fn get_event() {
    // SAFETY: Called on the SDK thread after the SDK was initialized.
    unsafe {
        sys::EdsGetEvent();
    }
}

/// An SDK object reference, released when dropped.
struct Ref(EdsBaseRef);

impl Ref {
    /// Takes ownership of a reference the SDK returned, if it is not null.
    fn new(object: EdsBaseRef) -> Option<Self> {
        (!object.is_null()).then_some(Self(object))
    }
}

impl Drop for Ref {
    fn drop(&mut self) {
        // SAFETY: The reference came from the SDK, is not null, and is owned
        // only by this `Ref`, so it is released exactly once.
        unsafe {
            sys::EdsRelease(self.0);
        }
    }
}

/// Creates an SDK object through `create`, which writes the new reference
/// into the pointer it is given.
fn create(create: impl FnOnce(*mut EdsBaseRef) -> EdsError) -> Result<Ref> {
    let mut object: EdsBaseRef = std::ptr::null_mut();
    check(create(&mut object))?;
    Ref::new(object).ok_or(Error::Sdk(sys::ERR_DEVICE_INVALID))
}

/// What a camera's event handlers have seen, waiting to be dealt with on the
/// SDK thread.
#[derive(Default)]
struct Events {
    /// Captured files the camera is waiting to send.
    transfers: Vec<Ref>,
    shut_down: bool,
    will_shut_down: bool,
}

extern "system" fn on_object(event: u32, object: EdsBaseRef, context: *mut c_void) -> EdsError {
    // The handler owns the reference it is given and must release it.
    let object = Ref::new(object);
    // SAFETY: `context` is the camera's boxed `Mutex<Events>`, which outlives
    // the handler's registration (see `Open`).
    let events = unsafe { &*context.cast::<Mutex<Events>>() };
    if event == sys::OBJECT_EVENT_DIR_ITEM_REQUEST_TRANSFER
        && let Some(object) = object
        && let Ok(mut events) = events.lock()
    {
        events.transfers.push(object);
    }
    sys::ERR_OK
}

extern "system" fn on_state(event: u32, _data: u32, context: *mut c_void) -> EdsError {
    // SAFETY: As in `on_object`.
    let events = unsafe { &*context.cast::<Mutex<Events>>() };
    if let Ok(mut events) = events.lock() {
        match event {
            sys::STATE_EVENT_SHUTDOWN => events.shut_down = true,
            sys::STATE_EVENT_WILL_SOON_SHUT_DOWN => events.will_shut_down = true,
            _ => {}
        }
    }
    sys::ERR_OK
}

/// A camera with an open session.
struct Open {
    port: String,
    camera: Ref,
    events: Box<Mutex<Events>>,
    session: bool,
    /// Where live view went before the computer asked for it.
    live_view_before: Option<u32>,
}

impl Open {
    fn new(camera: Ref, port: String) -> Result<Self> {
        let mut open = Self {
            port,
            camera,
            events: Box::default(),
            session: false,
            live_view_before: None,
        };
        let context = std::ptr::from_ref::<Mutex<Events>>(&open.events)
            .cast_mut()
            .cast::<c_void>();
        // SAFETY: The camera reference is alive, and `context` stays valid
        // until the handlers are removed in `Drop` (see the module docs).
        check(unsafe {
            sys::EdsSetObjectEventHandler(
                open.camera.0,
                sys::OBJECT_EVENT_ALL,
                Some(on_object),
                context,
            )
        })?;
        // SAFETY: As above.
        check(unsafe {
            sys::EdsSetCameraStateEventHandler(
                open.camera.0,
                sys::STATE_EVENT_ALL,
                Some(on_state),
                context,
            )
        })?;
        // SAFETY: The camera reference is alive.
        check(unsafe { sys::EdsOpenSession(open.camera.0) })?;
        open.session = true;

        // C3: pictures go to the computer, not the memory card. The camera
        // only takes pictures while it believes the computer has room.
        open.set(sys::PROP_SAVE_TO, sys::SAVE_TO_HOST)?;
        let capacity = sys::EdsCapacity {
            number_of_free_clusters: 0x7FFF_FFFF,
            bytes_per_sector: 0x1000,
            reset: 1,
        };
        // SAFETY: The session is open and the capacity is passed by value.
        retry_busy(|| unsafe { sys::EdsSetCapacity(open.camera.0, capacity) })?;
        Ok(open)
    }

    fn events(&self) -> std::sync::MutexGuard<'_, Events> {
        // The handlers never panic while holding the lock, but a poisoned
        // lock still holds usable events.
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn check_connected(&self) -> Result<()> {
        if self.events().shut_down {
            return Err(Error::Disconnected);
        }
        Ok(())
    }

    fn get(&self, property: u32) -> Result<u32> {
        self.check_connected()?;
        let mut value = 0u32;
        // SAFETY: The session is open and `value` is a buffer of the size
        // passed. Every property Brick reads is a 32 bit integer.
        retry_busy(|| unsafe {
            sys::EdsGetPropertyData(
                self.camera.0,
                property,
                0,
                size_of::<u32>() as u32,
                std::ptr::from_mut(&mut value).cast(),
            )
        })?;
        Ok(value)
    }

    fn set(&self, property: u32, value: u32) -> Result<()> {
        self.check_connected()?;
        // SAFETY: The session is open and `value` is a buffer of the size
        // passed. Every property Brick writes is a 32 bit integer.
        retry_busy(|| unsafe {
            sys::EdsSetPropertyData(
                self.camera.0,
                property,
                0,
                size_of::<u32>() as u32,
                std::ptr::from_ref(&value).cast(),
            )
        })
    }

    fn choices(&self, property: u32) -> Result<Vec<u32>> {
        self.check_connected()?;
        let mut desc = sys::EdsPropertyDesc {
            form: 0,
            access: 0,
            num_elements: 0,
            prop_desc: [0; 128],
        };
        // SAFETY: The session is open and `desc` has the layout of
        // `EdsPropertyDesc`.
        retry_busy(|| unsafe { sys::EdsGetPropertyDesc(self.camera.0, property, &mut desc) })?;
        let count = usize::try_from(desc.num_elements)
            .unwrap_or(0)
            .min(desc.prop_desc.len());
        // The codes are unsigned; the header only stores them as signed.
        Ok(desc.prop_desc[..count]
            .iter()
            .map(|code| code.cast_unsigned())
            .collect())
    }

    fn set_live_view(&mut self, on: bool) -> Result<()> {
        if on {
            // Older cameras need live view switched on first. Newer ones
            // reject this, which does not matter.
            let _ = self.set(sys::PROP_EVF_MODE, 1);
            let before = self.get(sys::PROP_EVF_OUTPUT_DEVICE)?;
            self.set(
                sys::PROP_EVF_OUTPUT_DEVICE,
                before | sys::EVF_OUTPUT_DEVICE_PC,
            )?;
            self.live_view_before.get_or_insert(before);
        } else if let Some(before) = self.live_view_before.take() {
            self.set(
                sys::PROP_EVF_OUTPUT_DEVICE,
                before & !sys::EVF_OUTPUT_DEVICE_PC,
            )?;
        }
        Ok(())
    }

    fn live_view_frame(&self) -> Result<Option<Vec<u8>>> {
        self.check_connected()?;
        // SAFETY: A size of zero makes a stream that grows as needed.
        let stream = create(|out| unsafe { sys::EdsCreateMemoryStream(0, out) })?;
        // SAFETY: The stream is alive.
        let image = create(|out| unsafe { sys::EdsCreateEvfImageRef(stream.0, out) })?;
        // SAFETY: The session is open and both references are alive.
        let error = unsafe { sys::EdsDownloadEvfImage(self.camera.0, image.0) };
        if error == sys::ERR_OBJECT_NOTREADY {
            return Ok(None);
        }
        check(error)?;
        read_stream(&stream).map(Some)
    }

    fn capture(&self, files: usize, timeout: Duration) -> Result<Vec<File>> {
        self.check_connected()?;
        // Files left over from an earlier capture that timed out are not
        // part of this one.
        self.cancel_transfers();
        // SAFETY: The session is open.
        let pressed = retry_busy(|| unsafe {
            sys::EdsSendCommand(
                self.camera.0,
                sys::COMMAND_PRESS_SHUTTER_BUTTON,
                sys::SHUTTER_BUTTON_COMPLETELY_NON_AF,
            )
        });
        // The button is let go even if pressing it failed, so the camera is
        // not left half pressed.
        // SAFETY: The session is open.
        let released = retry_busy(|| unsafe {
            sys::EdsSendCommand(
                self.camera.0,
                sys::COMMAND_PRESS_SHUTTER_BUTTON,
                sys::SHUTTER_BUTTON_OFF,
            )
        });
        pressed?;
        released?;

        let deadline = Instant::now() + timeout;
        let mut received = Vec::new();
        while received.len() < files {
            get_event();
            let transfers = std::mem::take(&mut self.events().transfers);
            for item in transfers {
                received.push(download(&item)?);
            }
            self.check_connected()?;
            if received.len() < files {
                if Instant::now() >= deadline {
                    return Err(Error::Timeout);
                }
                thread::sleep(CAPTURE_POLL);
            }
        }
        Ok(received)
    }

    /// Tells the camera that files it is waiting to send are not wanted.
    fn cancel_transfers(&self) {
        let transfers = std::mem::take(&mut self.events().transfers);
        for item in transfers {
            // SAFETY: The item reference is alive.
            unsafe {
                sys::EdsDownloadCancel(item.0);
            }
        }
    }

    /// Deals with events that arrived while no call was waiting for them.
    fn handle_events(&self) {
        let will_shut_down = std::mem::take(&mut self.events().will_shut_down);
        if will_shut_down {
            // Keeps the camera from turning itself off while it is in use.
            // SAFETY: The session is open.
            unsafe {
                sys::EdsSendCommand(self.camera.0, sys::COMMAND_EXTEND_SHUT_DOWN_TIMER, 0);
            }
        }
        // Pictures taken with the camera's own shutter button are not part
        // of any capture Brick asked for.
        self.cancel_transfers();
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        if self.session {
            let _ = self.set_live_view(false);
            // SAFETY: The session is open.
            unsafe {
                sys::EdsCloseSession(self.camera.0);
            }
        }
        // SAFETY: The camera reference is alive. Removing the handlers before
        // `events` is freed keeps the SDK from using a dangling context.
        unsafe {
            sys::EdsSetObjectEventHandler(
                self.camera.0,
                sys::OBJECT_EVENT_ALL,
                None,
                std::ptr::null_mut(),
            );
            sys::EdsSetCameraStateEventHandler(
                self.camera.0,
                sys::STATE_EVENT_ALL,
                None,
                std::ptr::null_mut(),
            );
        }
        self.cancel_transfers();
    }
}

/// Downloads a captured file the camera is waiting to send.
fn download(item: &Ref) -> Result<File> {
    let mut info = sys::EdsDirectoryItemInfo {
        size: 0,
        is_folder: 0,
        group_id: 0,
        option: 0,
        file_name: [0; sys::MAX_NAME],
        format: 0,
        date_time: 0,
    };
    // SAFETY: The item reference is alive and `info` has the layout of
    // `EdsDirectoryItemInfo`.
    check(unsafe { sys::EdsGetDirectoryItemInfo(item.0, &mut info) })?;
    let result = (|| {
        // SAFETY: Makes a stream with room for the whole file.
        let stream = create(|out| unsafe { sys::EdsCreateMemoryStream(info.size, out) })?;
        // SAFETY: The item and stream are alive, and the stream has room
        // for `info.size` bytes.
        check(unsafe { sys::EdsDownload(item.0, info.size, stream.0) })?;
        // SAFETY: The item is alive and was downloaded.
        check(unsafe { sys::EdsDownloadComplete(item.0) })?;
        read_stream(&stream)
    })();
    match result {
        Ok(data) => Ok(File {
            name: c_string(&info.file_name),
            data,
        }),
        Err(e) => {
            // SAFETY: The item reference is alive.
            unsafe {
                sys::EdsDownloadCancel(item.0);
            }
            Err(e)
        }
    }
}

/// Copies the contents of a memory stream.
fn read_stream(stream: &Ref) -> Result<Vec<u8>> {
    let mut length = 0u64;
    // SAFETY: The stream is alive.
    check(unsafe { sys::EdsGetLength(stream.0, &mut length) })?;
    if length == 0 {
        return Ok(Vec::new());
    }
    let mut pointer: *mut c_void = std::ptr::null_mut();
    // SAFETY: The stream is alive.
    check(unsafe { sys::EdsGetPointer(stream.0, &mut pointer) })?;
    let length = usize::try_from(length).map_err(|_| Error::Sdk(sys::ERR_DEVICE_INVALID))?;
    if pointer.is_null() {
        return Err(Error::Sdk(sys::ERR_DEVICE_INVALID));
    }
    // SAFETY: A memory stream's pointer is the start of its `length` bytes,
    // which stay valid while the stream is alive and unchanged, as it is
    // here.
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) };
    Ok(bytes.to_vec())
}

/// Reads a NUL terminated string from a fixed size buffer.
fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// The cameras listed by the SDK, with their device information.
fn list() -> Result<Vec<(Ref, DeviceInfo)>> {
    // SAFETY: Writes a new camera list reference.
    let list = create(|out| unsafe { sys::EdsGetCameraList(out) })?;
    let mut count = 0u32;
    // SAFETY: The list is alive.
    check(unsafe { sys::EdsGetChildCount(list.0, &mut count) })?;
    let mut cameras = Vec::new();
    for index in 0..i32::try_from(count).unwrap_or(0) {
        // SAFETY: The list is alive and `index` is below its child count.
        let Ok(camera) = create(|out| unsafe { sys::EdsGetChildAtIndex(list.0, index, out) })
        else {
            continue;
        };
        let mut info = sys::EdsDeviceInfo {
            port_name: [0; sys::MAX_NAME],
            device_description: [0; sys::MAX_NAME],
            device_sub_type: 0,
            reserved: 0,
        };
        // SAFETY: The camera reference is alive and `info` has the layout of
        // `EdsDeviceInfo`.
        if check(unsafe { sys::EdsGetDeviceInfo(camera.0, &mut info) }).is_ok() {
            let info = DeviceInfo {
                port: c_string(&info.port_name),
                description: c_string(&info.device_description),
            };
            cameras.push((camera, info));
        }
    }
    Ok(cameras)
}

/// Everything owned by the SDK thread.
#[derive(Default)]
struct State {
    cameras: HashMap<u64, Open>,
    next_id: u64,
}

impl State {
    fn camera(&mut self, id: u64) -> Result<&mut Open> {
        self.cameras.get_mut(&id).ok_or(Error::Disconnected)
    }

    fn handle_events(&mut self) {
        get_event();
        for camera in self.cameras.values() {
            camera.handle_events();
        }
    }
}

pub fn cameras() -> Result<Vec<DeviceInfo>> {
    call(|_| Ok(list()?.into_iter().map(|(_, info)| info).collect()))
}

pub fn open(port: &str) -> Result<u64> {
    let port = port.to_owned();
    call(move |state| {
        if state.cameras.values().any(|c| c.port == port) {
            return Err(Error::Sdk(sys::ERR_COMM_PORT_IS_IN_USE));
        }
        let camera = list()?
            .into_iter()
            .find(|(_, info)| info.port == port)
            .map(|(camera, _)| camera)
            .ok_or(Error::Disconnected)?;
        let open = Open::new(camera, port)?;
        state.next_id += 1;
        state.cameras.insert(state.next_id, open);
        Ok(state.next_id)
    })
}

pub fn close(camera: u64) {
    let _ = call(move |state| {
        state.cameras.remove(&camera);
        Ok(())
    });
}

pub fn get(camera: u64, property: u32) -> Result<u32> {
    call(move |state| state.camera(camera)?.get(property))
}

pub fn set(camera: u64, property: u32, value: u32) -> Result<()> {
    call(move |state| state.camera(camera)?.set(property, value))
}

pub fn choices(camera: u64, property: u32) -> Result<Vec<u32>> {
    call(move |state| state.camera(camera)?.choices(property))
}

pub fn set_live_view(camera: u64, on: bool) -> Result<()> {
    call(move |state| state.camera(camera)?.set_live_view(on))
}

pub fn live_view_frame(camera: u64) -> Result<Option<Vec<u8>>> {
    call(move |state| state.camera(camera)?.live_view_frame())
}

pub fn capture(camera: u64, files: usize, timeout: Duration) -> Result<Vec<File>> {
    call(move |state| state.camera(camera)?.capture(files, timeout))
}

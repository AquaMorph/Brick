//! Requests to a webcam's USB control endpoint through IOKit.
//!
//! AVFoundation does not let programs set a webcam's exposure, gain, white
//! balance, or focus, but USB Video Class webcams accept them as control
//! requests (C8, CT1). The device is never opened: opening it takes it away
//! from the system's webcam driver, which then stops sending frames until the
//! webcam is plugged in again. Control requests do not need it to be open.
//!
//! The soundness of the IOKit calls rests on these invariants:
//!
//! - The device interface pointer comes from `QueryInterface` with the UUID of
//!   `IOUSBDeviceInterface182`, so it points to a pointer to that vtable, whose
//!   layout `IOUSBDeviceStruct182` matches.
//! - The interface is released exactly once, when [`UsbDevice`] is dropped,
//!   and is not used after that.
//! - Every registry object handle is released once after its last use.

use std::ffi::c_void;
use std::io;
use std::ptr::{self, NonNull};
use std::thread;
use std::time::Duration;

use objc2_core_foundation::{CFNumber, CFRetained, CFString, CFUUID};
use objc2_io_kit::{
    IOCFPlugInInterface, IOCreatePlugInInterfaceForService, IODestroyPlugInInterface,
    IOIteratorNext, IOObjectRelease, IORegistryEntryCreateCFProperty, IOServiceGetMatchingServices,
    IOServiceMatching, IOUSBDevRequestTO, IOUSBDeviceInterface182, io_iterator_t, io_service_t,
    kIOReturnNoDevice, kIOReturnNoResources, kIOReturnNotResponding, kIOReturnTimeout,
    kIOUSBPipeStalled, kIOUSBTransactionTimeout,
};

/// How long a control request may take.
const TIMEOUT_MS: u32 = 1000;
/// `IOCreatePlugInInterfaceForService` sometimes fails with
/// `kIOReturnNoResources` for a moment, for example just after the device is
/// plugged in.
const PLUGIN_ATTEMPTS: usize = 5;

/// A USB device found by its location, which only allows requests on its
/// control endpoint.
pub struct UsbDevice {
    /// A pointer to the `IOUSBDeviceInterface182` vtable pointer.
    raw: NonNull<*mut IOUSBDeviceInterface182>,
}

// SAFETY: IOKit user client interfaces may be called from any thread.
// `UsbDevice` is not `Sync`, so calls never happen at the same time.
unsafe impl Send for UsbDevice {}

impl UsbDevice {
    /// The USB device at `location_id`, if it has the given vendor and
    /// product IDs.
    pub fn find(location_id: u32, vendor_id: u16, product_id: u16) -> io::Result<UsbDevice> {
        let service = find_service(location_id, vendor_id, product_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such USB device"))?;
        let device = open_interface(service.0);
        drop(service);
        device
    }

    /// A request that reads up to `length` bytes from the device.
    pub fn control_in(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    ) -> io::Result<Vec<u8>> {
        let mut data = vec![0u8; usize::from(length)];
        let done = self.request(
            request_type | 0x80,
            request,
            value,
            index,
            data.as_mut_ptr(),
            length,
        )?;
        data.truncate(done);
        Ok(data)
    }

    /// A request that writes `data` to the device.
    pub fn control_out(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> io::Result<()> {
        let length = u16::try_from(data.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "request too long"))?;
        // IOKit takes a mutable pointer but does not write to the buffer of an
        // OUT request. A copy keeps that from mattering.
        let mut data = data.to_vec();
        self.request(
            request_type & 0x7f,
            request,
            value,
            index,
            data.as_mut_ptr(),
            length,
        )
        .map(|_| ())
    }

    /// Sends a request with `length` bytes at `data`, returning how many
    /// bytes were transferred.
    fn request(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: *mut u8,
        length: u16,
    ) -> io::Result<usize> {
        let mut req = IOUSBDevRequestTO {
            bmRequestType: request_type,
            bRequest: request,
            wValue: value,
            wIndex: index,
            wLength: length,
            pData: data.cast::<c_void>(),
            wLenDone: 0,
            noDataTimeout: TIMEOUT_MS,
            completionTimeout: TIMEOUT_MS,
        };
        let this = self.raw.as_ptr();
        // SAFETY: `this` is a live device interface (see the module
        // invariants). `req` points to `length` writable bytes at `data` and
        // lives for the whole synchronous call.
        let result = unsafe {
            let call = (**this)
                .DeviceRequestTO
                .ok_or_else(|| io::Error::other("the USB device cannot send requests"))?;
            call(this.cast::<c_void>(), &mut req)
        };
        check(result)?;
        Ok((req.wLenDone as usize).min(usize::from(length)))
    }
}

impl Drop for UsbDevice {
    fn drop(&mut self) {
        let this = self.raw.as_ptr();
        // SAFETY: the interface is live and released only here.
        unsafe {
            if let Some(release) = (**this).Release {
                release(this.cast::<c_void>());
            }
        }
    }
}

/// A registry object handle, released when dropped.
struct Object(io_service_t);

impl Drop for Object {
    fn drop(&mut self) {
        IOObjectRelease(self.0);
    }
}

fn find_service(location_id: u32, vendor_id: u16, product_id: u16) -> Option<Object> {
    // SAFETY: the class name is a valid C string.
    let matching = unsafe { IOServiceMatching(c"IOUSBHostDevice".as_ptr()) }?;
    // SAFETY: a mutable dictionary is a dictionary.
    let matching = unsafe { CFRetained::cast_unchecked(matching) };
    let mut iterator: io_iterator_t = 0;
    // SAFETY: port 0 is the default main port, which also works on systems
    // older than `kIOMainPortDefault`. The call consumes `matching`, and
    // `iterator` is a valid place for the result.
    let result = unsafe { IOServiceGetMatchingServices(0, Some(matching), &mut iterator) };
    if result != 0 || iterator == 0 {
        return None;
    }
    let iterator = Object(iterator);
    loop {
        let service = IOIteratorNext(iterator.0);
        if service == 0 {
            return None;
        }
        let service = Object(service);
        let matches = number(service.0, "locationID") == Some(i64::from(location_id))
            && number(service.0, "idVendor") == Some(i64::from(vendor_id))
            && number(service.0, "idProduct") == Some(i64::from(product_id));
        if matches {
            return Some(service);
        }
    }
}

/// A number property of a registry entry.
fn number(service: io_service_t, key: &'static str) -> Option<i64> {
    let key = CFString::from_static_str(key);
    // SAFETY: `service` is a live handle and `key` a valid string. The
    // default allocator is used and no options are defined.
    let value = unsafe { IORegistryEntryCreateCFProperty(service, Some(&key), None, 0) }?;
    value.downcast::<CFNumber>().ok()?.as_i64()
}

fn open_interface(service: io_service_t) -> io::Result<UsbDevice> {
    let mut plugin: *mut *mut IOCFPlugInInterface = ptr::null_mut();
    let mut result = 0;
    for _ in 0..PLUGIN_ATTEMPTS {
        let mut score = 0;
        // SAFETY: `service` is a live handle, the UUIDs are the documented
        // device user client and plug-in interface types, and both out
        // pointers are valid.
        result = unsafe {
            IOCreatePlugInInterfaceForService(
                service,
                Some(&device_user_client_type()),
                Some(&plugin_interface()),
                &mut plugin,
                &mut score,
            )
        };
        if result as u32 != kIOReturnNoResources {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    check(result)?;
    if plugin.is_null() {
        return Err(io::Error::other("IOKit gave no USB plug-in"));
    }

    let mut device: *mut *mut IOUSBDeviceInterface182 = ptr::null_mut();
    // SAFETY: `plugin` is the live plug-in interface created above. Its
    // vtable has `IOCFPlugInInterface`'s layout, and `device` is a valid place
    // for the returned interface. The plug-in is destroyed once, after its
    // last use; the device interface holds its own reference.
    let queried = unsafe {
        let query = (**plugin).QueryInterface;
        let queried = query.map(|query| {
            query(
                plugin.cast::<c_void>(),
                device_interface().uuid_bytes(),
                (&raw mut device).cast(),
            )
        });
        IODestroyPlugInInterface(plugin);
        queried
    };
    match (queried, NonNull::new(device)) {
        (Some(0), Some(raw)) if !vtable_is_null(raw) => Ok(UsbDevice { raw }),
        _ => Err(io::Error::other("the USB device has no device interface")),
    }
}

/// Whether the interface's vtable pointer is null, which IOKit should never
/// return.
fn vtable_is_null(raw: NonNull<*mut IOUSBDeviceInterface182>) -> bool {
    // SAFETY: `raw` points to the interface pointer `QueryInterface` wrote.
    unsafe { raw.as_ptr().read().is_null() }
}

fn check(result: i32) -> io::Result<()> {
    let code = result as u32;
    let (kind, message) = if code == 0 {
        return Ok(());
    } else if code == kIOReturnNoDevice || code == kIOReturnNotResponding {
        (
            io::ErrorKind::NotConnected,
            "the USB device was disconnected",
        )
    } else if code == kIOUSBPipeStalled {
        (
            io::ErrorKind::Unsupported,
            "the USB device refused the request",
        )
    } else if code == kIOReturnTimeout || code == kIOUSBTransactionTimeout {
        (io::ErrorKind::TimedOut, "the USB device did not answer")
    } else {
        return Err(io::Error::other(format!(
            "USB request failed with IOKit error {code:#010x}"
        )));
    };
    Err(io::Error::new(kind, message))
}

fn uuid(bytes: [u8; 16]) -> CFRetained<CFUUID> {
    let [
        b0,
        b1,
        b2,
        b3,
        b4,
        b5,
        b6,
        b7,
        b8,
        b9,
        b10,
        b11,
        b12,
        b13,
        b14,
        b15,
    ] = bytes;
    CFUUID::constant_uuid_with_bytes(
        None, b0, b1, b2, b3, b4, b5, b6, b7, b8, b9, b10, b11, b12, b13, b14, b15,
    )
    .expect("CoreFoundation creates constant UUIDs")
}

/// `kIOUSBDeviceUserClientTypeID` from `IOUSBLib.h`.
fn device_user_client_type() -> CFRetained<CFUUID> {
    uuid([
        0x9d, 0xc7, 0xb7, 0x80, 0x9e, 0xc0, 0x11, 0xd4, 0xa5, 0x4f, 0x00, 0x0a, 0x27, 0x05, 0x28,
        0x61,
    ])
}

/// `kIOCFPlugInInterfaceID` from `IOCFPlugIn.h`.
fn plugin_interface() -> CFRetained<CFUUID> {
    uuid([
        0xc2, 0x44, 0xe8, 0x58, 0x10, 0x9c, 0x11, 0xd4, 0x91, 0xd4, 0x00, 0x50, 0xe4, 0xc6, 0x42,
        0x6f,
    ])
}

/// `kIOUSBDeviceInterfaceID182` from `IOUSBLib.h`, the first version with
/// `DeviceRequestTO`.
fn device_interface() -> CFRetained<CFUUID> {
    uuid([
        0x15, 0x2f, 0xc4, 0x96, 0x48, 0x91, 0x11, 0xd5, 0x9d, 0x52, 0x00, 0x0a, 0x27, 0x80, 0x1e,
        0x86,
    ])
}

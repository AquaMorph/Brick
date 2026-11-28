//! The parts of the EDSDK C API (`EDSDK.h`, `EDSDKTypes.h`, `EDSDKErrors.h`)
//! Brick uses, written out by hand from Canon's headers.
//!
//! Every struct mirrors Canon's struct of the same name field for field, with
//! `#[repr(C)]`. Canon's headers pack structs to 8 bytes, which is the natural
//! alignment of every field here, so the field offsets match. The sizes are
//! checked at compile time.

use std::ffi::c_void;

pub type EdsError = u32;
pub type EdsBaseRef = *mut c_void;
pub type EdsPropertyId = u32;

pub const MAX_NAME: usize = 256;

#[repr(C)]
pub struct EdsDeviceInfo {
    pub port_name: [u8; MAX_NAME],
    pub device_description: [u8; MAX_NAME],
    pub device_sub_type: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct EdsDirectoryItemInfo {
    pub size: u64,
    pub is_folder: i32,
    pub group_id: u32,
    pub option: u32,
    pub file_name: [u8; MAX_NAME],
    pub format: u32,
    pub date_time: u32,
}

#[repr(C)]
pub struct EdsPropertyDesc {
    pub form: i32,
    pub access: i32,
    pub num_elements: i32,
    pub prop_desc: [i32; 128],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EdsCapacity {
    pub number_of_free_clusters: i32,
    pub bytes_per_sector: i32,
    pub reset: i32,
}

const _: () = {
    assert!(size_of::<EdsDeviceInfo>() == 520);
    assert!(size_of::<EdsDirectoryItemInfo>() == 288);
    assert!(size_of::<EdsPropertyDesc>() == 524);
    assert!(size_of::<EdsCapacity>() == 12);
};

pub type EdsObjectEventHandler =
    extern "system" fn(event: u32, item: EdsBaseRef, context: *mut c_void) -> EdsError;
pub type EdsStateEventHandler =
    extern "system" fn(event: u32, data: u32, context: *mut c_void) -> EdsError;

// Errors.
pub const ERR_OK: EdsError = 0x0000_0000;
pub const ERR_DEVICE_NOT_FOUND: EdsError = 0x0000_0080;
pub const ERR_DEVICE_BUSY: EdsError = 0x0000_0081;
pub const ERR_DEVICE_INVALID: EdsError = 0x0000_0082;
pub const ERR_COMM_PORT_IS_IN_USE: EdsError = 0x0000_00C0;
pub const ERR_COMM_DISCONNECTED: EdsError = 0x0000_00C1;
pub const ERR_COMM_USB_BUS_ERR: EdsError = 0x0000_00C4;
pub const ERR_SESSION_NOT_OPEN: EdsError = 0x0000_2003;
pub const ERR_OBJECT_NOTREADY: EdsError = 0x0000_A102;

// Properties.
pub const PROP_SAVE_TO: EdsPropertyId = 0x0000_000b;
pub const PROP_EVF_OUTPUT_DEVICE: EdsPropertyId = 0x0000_0500;
pub const PROP_EVF_MODE: EdsPropertyId = 0x0000_0501;

pub const SAVE_TO_HOST: u32 = 2;
pub const EVF_OUTPUT_DEVICE_PC: u32 = 2;

// Commands.
pub const COMMAND_EXTEND_SHUT_DOWN_TIMER: u32 = 0x0000_0001;
pub const COMMAND_PRESS_SHUTTER_BUTTON: u32 = 0x0000_0004;
pub const SHUTTER_BUTTON_OFF: i32 = 0x0000_0000;
pub const SHUTTER_BUTTON_COMPLETELY_NON_AF: i32 = 0x0001_0003;

// Events.
pub const OBJECT_EVENT_ALL: u32 = 0x0000_0200;
pub const OBJECT_EVENT_DIR_ITEM_REQUEST_TRANSFER: u32 = 0x0000_0208;
pub const STATE_EVENT_ALL: u32 = 0x0000_0300;
pub const STATE_EVENT_SHUTDOWN: u32 = 0x0000_0301;
pub const STATE_EVENT_WILL_SOON_SHUT_DOWN: u32 = 0x0000_0303;

#[cfg_attr(target_os = "macos", link(name = "EDSDK", kind = "framework"))]
#[cfg_attr(not(target_os = "macos"), link(name = "EDSDK"))]
unsafe extern "system" {
    pub fn EdsInitializeSDK() -> EdsError;
    pub fn EdsTerminateSDK() -> EdsError;
    pub fn EdsRelease(object: EdsBaseRef) -> u32;
    pub fn EdsGetChildCount(object: EdsBaseRef, count: *mut u32) -> EdsError;
    pub fn EdsGetChildAtIndex(object: EdsBaseRef, index: i32, child: *mut EdsBaseRef) -> EdsError;
    pub fn EdsGetPropertyData(
        object: EdsBaseRef,
        property: EdsPropertyId,
        param: i32,
        size: u32,
        data: *mut c_void,
    ) -> EdsError;
    pub fn EdsSetPropertyData(
        object: EdsBaseRef,
        property: EdsPropertyId,
        param: i32,
        size: u32,
        data: *const c_void,
    ) -> EdsError;
    pub fn EdsGetPropertyDesc(
        object: EdsBaseRef,
        property: EdsPropertyId,
        desc: *mut EdsPropertyDesc,
    ) -> EdsError;
    pub fn EdsGetCameraList(list: *mut EdsBaseRef) -> EdsError;
    pub fn EdsGetDeviceInfo(camera: EdsBaseRef, info: *mut EdsDeviceInfo) -> EdsError;
    pub fn EdsOpenSession(camera: EdsBaseRef) -> EdsError;
    pub fn EdsCloseSession(camera: EdsBaseRef) -> EdsError;
    pub fn EdsSendCommand(camera: EdsBaseRef, command: u32, param: i32) -> EdsError;
    pub fn EdsSetCapacity(camera: EdsBaseRef, capacity: EdsCapacity) -> EdsError;
    pub fn EdsGetDirectoryItemInfo(item: EdsBaseRef, info: *mut EdsDirectoryItemInfo) -> EdsError;
    pub fn EdsDownload(item: EdsBaseRef, size: u64, stream: EdsBaseRef) -> EdsError;
    pub fn EdsDownloadComplete(item: EdsBaseRef) -> EdsError;
    pub fn EdsDownloadCancel(item: EdsBaseRef) -> EdsError;
    pub fn EdsCreateMemoryStream(size: u64, stream: *mut EdsBaseRef) -> EdsError;
    pub fn EdsGetPointer(stream: EdsBaseRef, pointer: *mut *mut c_void) -> EdsError;
    pub fn EdsGetLength(stream: EdsBaseRef, length: *mut u64) -> EdsError;
    pub fn EdsCreateEvfImageRef(stream: EdsBaseRef, image: *mut EdsBaseRef) -> EdsError;
    pub fn EdsDownloadEvfImage(camera: EdsBaseRef, image: EdsBaseRef) -> EdsError;
    pub fn EdsSetObjectEventHandler(
        camera: EdsBaseRef,
        event: u32,
        handler: Option<EdsObjectEventHandler>,
        context: *mut c_void,
    ) -> EdsError;
    pub fn EdsSetCameraStateEventHandler(
        camera: EdsBaseRef,
        event: u32,
        handler: Option<EdsStateEventHandler>,
        context: *mut c_void,
    ) -> EdsError;
    pub fn EdsGetEvent() -> EdsError;
}

#[cfg(target_os = "windows")]
pub const COINIT_APARTMENTTHREADED: u32 = 0x2;

#[cfg(target_os = "windows")]
#[link(name = "ole32")]
unsafe extern "system" {
    pub fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    pub fn CoUninitialize();
}

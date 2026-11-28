//! Safe access to Canon DSLRs through Canon's EOS Digital SDK (EDSDK) (C3).
//!
//! This crate holds all of the unsafe code needed to talk to the EDSDK (R4).
//! The rest of Brick only sees the safe API: [`cameras`] and [`Camera`].
//!
//! The EDSDK cannot be distributed with Brick (R10), so the build script
//! links it only when it has been installed with `cargo xtask edsdk`. Without
//! it, [`AVAILABLE`] is `false` and every call fails with
//! [`Error::NotInstalled`] (R11).
//!
//! The SDK is not safe to call from several threads at once and delivers
//! camera events only to the thread that asks for them, so every SDK call is
//! made on one thread that this crate starts on first use. The functions here
//! send their work to that thread and wait for the answer.

#[cfg(brick_edsdk)]
mod sdk;
#[cfg(brick_edsdk)]
mod sys;

#[cfg(not(brick_edsdk))]
#[path = "missing.rs"]
mod sdk;

use std::fmt;
use std::time::Duration;

/// Whether this build was linked with the EDSDK.
pub const AVAILABLE: bool = cfg!(brick_edsdk);

/// A camera property read or written as a 32 bit code. What each code means
/// is listed in Canon's EDSDK API reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Property {
    /// The image size and format, such as RAW + Large Fine JPEG.
    ImageQuality,
    /// The shooting mode on the mode dial, such as Manual.
    AeMode,
    Iso,
    Av,
    Tv,
}

impl Property {
    fn id(self) -> u32 {
        match self {
            Property::ImageQuality => 0x0000_0100,
            Property::AeMode => 0x0000_0400,
            Property::Iso => 0x0000_0402,
            Property::Av => 0x0000_0405,
            Property::Tv => 0x0000_0406,
        }
    }
}

/// A connected camera found by [`cameras`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Where the camera is connected. Changes when it is plugged in again.
    pub port: String,
    /// The camera's model name, such as "Canon EOS R6".
    pub description: String,
}

/// A file sent by the camera after a capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// The name the camera gave the file, such as `IMG_0001.JPG`.
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// This build does not include the EDSDK (R11).
    NotInstalled,
    /// The SDK could not be started or its thread stopped.
    Stopped(String),
    /// The camera was disconnected or turned off.
    Disconnected,
    /// The camera did not answer in time.
    Timeout,
    /// An SDK error code from `EDSDKErrors.h`.
    Sdk(u32),
}

impl Error {
    pub fn is_disconnected(&self) -> bool {
        matches!(self, Error::Disconnected)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotInstalled => f.write_str("This build of Brick has no Canon DSLR support"),
            Error::Stopped(reason) => write!(f, "The Canon SDK stopped: {reason}"),
            Error::Disconnected => f.write_str("The camera was disconnected"),
            Error::Timeout => f.write_str("The camera did not answer in time"),
            Error::Sdk(code) => f.write_str(&sdk_message(*code)),
        }
    }
}

impl std::error::Error for Error {}

/// A readable message for the SDK errors a user can do something about.
fn sdk_message(code: u32) -> String {
    match code {
        0x0000_0081 => "The camera is busy".into(),
        0x0000_00C0 => {
            "The camera is in use by another program, such as EOS Utility or Image Capture".into()
        }
        0x0000_8D01 => "The camera could not focus".into(),
        0x0000_8D03 => "The camera's mirror is locked up".into(),
        0x0000_8D04 => "The camera is cleaning its sensor".into(),
        0x0000_8D06 => "The camera needs a memory card to take pictures".into(),
        0x0000_8D07 => "The camera's memory card has a problem".into(),
        0x0000_8D08 => "The camera's memory card is write protected".into(),
        0x0000_8D0B => "No lens is attached to the camera".into(),
        _ => format!("Canon SDK error 0x{code:08X}"),
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// The Canon cameras currently connected.
pub fn cameras() -> Result<Vec<DeviceInfo>> {
    sdk::cameras()
}

/// An open camera. Dropping it closes the camera's session.
#[derive(Debug)]
pub struct Camera {
    id: u64,
}

impl Camera {
    /// Opens the camera connected at `port` ([`DeviceInfo::port`]). Pictures
    /// are sent to the computer and not saved on the camera's memory card
    /// (C3).
    pub fn open(port: &str) -> Result<Self> {
        sdk::open(port).map(|id| Self { id })
    }

    pub fn get(&self, property: Property) -> Result<u32> {
        sdk::get(self.id, property.id())
    }

    pub fn set(&self, property: Property, value: u32) -> Result<()> {
        sdk::set(self.id, property.id(), value)
    }

    /// The values the camera accepts for `property` right now. These change
    /// with the mode dial and the lens.
    pub fn choices(&self, property: Property) -> Result<Vec<u32>> {
        sdk::choices(self.id, property.id())
    }

    /// Starts or stops sending live view to the computer.
    pub fn set_live_view(&self, on: bool) -> Result<()> {
        sdk::set_live_view(self.id, on)
    }

    /// The newest live view image as a JPEG, or `None` if the camera has no
    /// new image ready yet.
    pub fn live_view_frame(&self) -> Result<Option<Vec<u8>>> {
        sdk::live_view_frame(self.id)
    }

    /// Takes a picture without autofocus and waits up to `timeout` for the
    /// camera to send `files` files, such as two for RAW + JPEG.
    pub fn capture(&self, files: usize, timeout: Duration) -> Result<Vec<File>> {
        sdk::capture(self.id, files, timeout)
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        sdk::close(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_explain_themselves() {
        assert_eq!(
            Error::Sdk(0x8D0B).to_string(),
            "No lens is attached to the camera"
        );
        assert_eq!(Error::Sdk(0x1234).to_string(), "Canon SDK error 0x00001234");
        assert!(Error::Disconnected.is_disconnected());
        assert!(!Error::Timeout.is_disconnected());
    }

    #[test]
    #[cfg(brick_edsdk)]
    fn the_sdk_starts() {
        // Lists whatever cameras are connected, which is none without
        // hardware.
        assert!(cameras().is_ok());
        assert_eq!(
            Camera::open("no such port").unwrap_err(),
            Error::Disconnected
        );
    }

    #[test]
    #[cfg(not(brick_edsdk))]
    fn without_the_sdk_nothing_is_found() {
        assert_eq!(cameras(), Err(Error::NotInstalled));
        assert_eq!(Camera::open("usb").unwrap_err(), Error::NotInstalled);
    }
}

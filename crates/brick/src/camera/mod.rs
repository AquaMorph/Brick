//! The camera abstraction shared by DSLRs and webcams (C1).
//!
//! A [`Backend`] finds cameras of one kind and opens them. An opened
//! [`Camera`] gives live view frames, captures images, and reads and writes
//! settings. Cameras are only ever driven from their own worker thread
//! ([`worker`]), so a slow camera never blocks the UI (AT12).

pub mod manager;
#[cfg(debug_assertions)]
mod simulated;
#[cfg(target_os = "linux")]
mod v4l2;
mod worker;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use manager::{CameraEvent, Cameras, Connection};

/// A camera setting the user can change on the Cinematography tab (CT1).
/// Each camera offers the ones it supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Setting {
    /// The size of captured frames, such as `1920x1080`. The camera's native
    /// resolution by default (C10).
    Resolution,
    ShutterSpeed,
    Aperture,
    Iso,
    Gain,
    WhiteBalance,
    Focus,
}

impl Setting {
    pub const ALL: [Setting; 7] = [
        Setting::Resolution,
        Setting::ShutterSpeed,
        Setting::Aperture,
        Setting::Iso,
        Setting::Gain,
        Setting::WhiteBalance,
        Setting::Focus,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Setting::Resolution => "Resolution",
            Setting::ShutterSpeed => "Shutter speed",
            Setting::Aperture => "Aperture",
            Setting::Iso => "ISO",
            Setting::Gain => "Gain",
            Setting::WhiteBalance => "White balance",
            Setting::Focus => "Focus",
        }
    }
}

/// The values of a camera's settings, as the camera names them. Saved per
/// shot in `shot.conf` (CT7, P12). A setting that is `None` is left as the
/// camera has it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shutter_speed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aperture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iso: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub white_balance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
}

impl CameraSettings {
    pub fn get(&self, setting: Setting) -> Option<&str> {
        self.slot(setting).as_deref()
    }

    pub fn set(&mut self, setting: Setting, value: Option<String>) {
        *self.slot_mut(setting) = value;
    }

    pub fn is_empty(&self) -> bool {
        Setting::ALL.iter().all(|s| self.get(*s).is_none())
    }

    fn slot(&self, setting: Setting) -> &Option<String> {
        match setting {
            Setting::Resolution => &self.resolution,
            Setting::ShutterSpeed => &self.shutter_speed,
            Setting::Aperture => &self.aperture,
            Setting::Iso => &self.iso,
            Setting::Gain => &self.gain,
            Setting::WhiteBalance => &self.white_balance,
            Setting::Focus => &self.focus,
        }
    }

    fn slot_mut(&mut self, setting: Setting) -> &mut Option<String> {
        match setting {
            Setting::Resolution => &mut self.resolution,
            Setting::ShutterSpeed => &mut self.shutter_speed,
            Setting::Aperture => &mut self.aperture,
            Setting::Iso => &mut self.iso,
            Setting::Gain => &mut self.gain,
            Setting::WhiteBalance => &mut self.white_balance,
            Setting::Focus => &mut self.focus,
        }
    }
}

/// The label of a resolution setting.
pub fn resolution_label(width: u32, height: u32) -> String {
    format!("{width}x{height}")
}

/// A setting the camera supports and the values it accepts, in the order the
/// camera lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub setting: Setting,
    pub choices: Vec<String>,
}

/// A detected camera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraInfo {
    /// Stays the same across runs and reconnects, so the last used camera can
    /// be found again (C5).
    pub id: String,
    pub name: String,
}

/// One live view image, 8 bit RGBA without premultiplied alpha.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The files produced by one capture.
pub struct Capture {
    pub jpeg: Vec<u8>,
    /// The RAW file and its extension without the dot, when the camera
    /// captures RAW (C4).
    pub raw: Option<(String, Vec<u8>)>,
}

#[derive(Debug, Clone)]
pub enum Error {
    /// The camera is gone. The worker stops and the camera must be reopened.
    #[cfg_attr(
        all(not(target_os = "linux"), not(test)),
        expect(
            dead_code,
            reason = "only the V4L2 backend can lose its camera until the other platform backends exist"
        )
    )]
    Disconnected(String),
    /// A request failed but the camera is still usable.
    Failed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Disconnected(message) | Error::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// An open camera (C1).
pub trait Camera: Send {
    /// The settings the camera supports and their accepted values.
    fn controls(&self) -> Vec<Control>;

    /// The camera's current settings.
    fn settings(&self) -> CameraSettings;

    /// Changes one setting to one of its [`Control::choices`].
    fn set(&mut self, setting: Setting, value: &str) -> Result<()>;

    /// Starts or stops the live view stream.
    fn set_live_view(&mut self, on: bool) -> Result<()>;

    /// Waits for the next live view frame.
    fn live_view(&mut self) -> Result<Frame>;

    /// Takes a picture and transfers it to the computer (C3).
    fn capture(&mut self) -> Result<Capture>;
}

/// Finds and opens cameras of one kind, such as webcams on this platform or
/// Canon DSLRs.
pub trait Backend: Send {
    /// The cameras currently connected.
    fn detect(&mut self) -> Vec<CameraInfo>;

    fn open(&mut self, id: &str) -> Result<Box<dyn Camera>>;
}

/// Every backend compiled into this build.
// Each backend is compiled in conditionally, so a `vec![]` literal cannot list
// them.
#[allow(unused_mut, clippy::vec_init_then_push)]
pub fn backends() -> Vec<Box<dyn Backend>> {
    let mut backends: Vec<Box<dyn Backend>> = Vec::new();
    #[cfg(target_os = "linux")]
    backends.push(Box::new(v4l2::V4l2Backend));
    #[cfg(debug_assertions)]
    backends.push(Box::new(simulated::SimulatedBackend));
    backends
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_toml() {
        let mut settings = CameraSettings::default();
        assert!(settings.is_empty());
        settings.set(Setting::ShutterSpeed, Some("1/60".into()));
        settings.set(Setting::Iso, Some("200".into()));
        let text = toml::to_string(&settings).unwrap();
        assert_eq!(text, "shutter_speed = \"1/60\"\niso = \"200\"\n");
        let back: CameraSettings = toml::from_str(&text).unwrap();
        assert_eq!(back, settings);
        assert_eq!(back.get(Setting::Aperture), None);
    }
}

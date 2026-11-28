//! Webcams on macOS through AVFoundation (C2).
//!
//! AVFoundation on macOS does not let programs set a webcam's exposure time,
//! gain, white balance, or focus, so only the resolution is offered (CT1).
//! Instead, once the automatic exposure, white balance, and focus have
//! settled after opening, they are locked where the webcam allows it, so the
//! picture does not flicker between frames (C8).

use std::io::{self, Cursor};
use std::time::Duration;

use brick_avfoundation::{self as avf, Device};
use image::codecs::jpeg::JpegEncoder;

use super::{Backend, Camera, CameraInfo, CameraSettings, Capture, Control, Error, Frame, Result};
use super::{Setting, resolution_label};

const ID_PREFIX: &str = "avfoundation:";
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);
/// Frames dropped after the camera starts for a capture, while the sensor
/// settles.
const WARM_UP_FRAMES: usize = 3;
/// Frames read after opening before the automatic settings are locked, so
/// they have time to settle.
const SETTLE_FRAMES: usize = 15;
const JPEG_QUALITY: u8 = 95;

pub struct AvFoundationBackend;

impl Backend for AvFoundationBackend {
    fn detect(&mut self) -> Vec<CameraInfo> {
        avf::devices().iter().map(camera_info).collect()
    }

    fn open(&mut self, id: &str) -> Result<Box<dyn Camera>> {
        let unique_id = id
            .strip_prefix(ID_PREFIX)
            .ok_or_else(|| Error::Failed("The webcam is not connected.".into()))?;
        let device = Device::open(unique_id).map_err(|e| error(&e, "Could not open the webcam"))?;
        Ok(Box::new(Webcam::open(device)?))
    }
}

/// AVFoundation's unique ID stays the same across reconnects and reboots,
/// so the webcam is found again after it is plugged back in (C5).
fn camera_info(device: &avf::DeviceInfo) -> CameraInfo {
    CameraInfo {
        id: format!("{ID_PREFIX}{}", device.unique_id),
        name: device.name.clone(),
    }
}

struct Webcam {
    device: Device,
}

impl Webcam {
    fn open(mut device: Device) -> Result<Self> {
        // C8: let the automatic settings find the scene, then fix them.
        device
            .set_running(true)
            .map_err(|e| error(&e, "Could not start the webcam"))?;
        for _ in 0..SETTLE_FRAMES {
            if device.next_frame(FRAME_TIMEOUT).is_err() {
                break;
            }
        }
        // Not every webcam allows fixing these; it is still usable if not.
        let _ = device.lock_automatic();
        device
            .set_running(false)
            .map_err(|e| error(&e, "Could not stop the webcam"))?;
        Ok(Self { device })
    }

    fn set_resolution(&mut self, value: &str) -> Result<()> {
        let (width, height) = self
            .device
            .sizes()
            .into_iter()
            .find(|(w, h)| resolution_label(*w, *h) == value)
            .ok_or_else(|| {
                Error::Failed(format!(
                    "This webcam does not support the resolution {value}."
                ))
            })?;
        self.device
            .set_size(width, height)
            .map_err(|e| error(&e, "The webcam refused the resolution"))
    }
}

impl Camera for Webcam {
    fn controls(&self) -> Vec<Control> {
        vec![Control {
            setting: Setting::Resolution,
            choices: self
                .device
                .sizes()
                .into_iter()
                .map(|(w, h)| resolution_label(w, h))
                .collect(),
        }]
    }

    fn settings(&self) -> CameraSettings {
        let (width, height) = self.device.size();
        let mut settings = CameraSettings::default();
        settings.set(Setting::Resolution, Some(resolution_label(width, height)));
        settings
    }

    fn set(&mut self, setting: Setting, value: &str) -> Result<()> {
        match setting {
            Setting::Resolution => self.set_resolution(value),
            _ => Err(Error::Failed(format!(
                "This webcam does not support {} {value}.",
                setting.label()
            ))),
        }
    }

    fn set_live_view(&mut self, on: bool) -> Result<()> {
        // Stopping turns the webcam's light off.
        self.device
            .set_running(on)
            .map_err(|e| error(&e, "Could not start the webcam"))
    }

    fn live_view(&mut self) -> Result<Frame> {
        if !self.device.is_running() {
            return Err(Error::Failed("Live view is off.".into()));
        }
        // Older frames are dropped so live view never lags (CT2).
        let image = self
            .device
            .latest_frame(FRAME_TIMEOUT)
            .map_err(|e| error(&e, "No live view frame"))?;
        Ok(Frame {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        })
    }

    /// Webcams have no shutter: a capture is the next frame the webcam sends.
    /// Frames already waiting were taken before the capture was asked for,
    /// possibly with the animator's hand still in the shot, so they are
    /// skipped.
    fn capture(&mut self) -> Result<Capture> {
        let was_running = self.device.is_running();
        if !was_running {
            self.set_live_view(true)?;
        }
        let result = self.take(if was_running { 0 } else { WARM_UP_FRAMES });
        if !was_running {
            let _ = self.device.set_running(false);
        }
        let image = result?;

        let rgb = image::RgbaImage::from_raw(image.width, image.height, image.rgba)
            .map(|rgba| image::DynamicImage::ImageRgba8(rgba).into_rgb8())
            .ok_or_else(|| Error::Failed("The webcam sent an unreadable frame.".into()))?;
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), JPEG_QUALITY)
            .encode_image(&rgb)
            .map_err(|e| Error::Failed(format!("Could not encode the image: {e}")))?;
        Ok(Capture { jpeg, raw: None })
    }
}

impl Webcam {
    /// The next frame after dropping `skip` frames.
    fn take(&mut self, skip: usize) -> Result<avf::Image> {
        let mut read = || {
            self.device
                .next_frame(FRAME_TIMEOUT)
                .map_err(|e| error(&e, "The webcam sent no picture"))
        };
        for _ in 0..skip {
            read()?;
        }
        read()
    }
}

fn error(e: &io::Error, context: &str) -> Error {
    if avf::is_disconnected(e) {
        Error::Disconnected("The webcam was disconnected.".into())
    } else if e.kind() == io::ErrorKind::PermissionDenied {
        Error::Failed(format!("{e}."))
    } else {
        Error::Failed(format!("{context}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_keep_the_unique_id() {
        let info = camera_info(&avf::DeviceInfo {
            unique_id: "0x1234".into(),
            name: "USB Camera".into(),
        });
        assert_eq!(info.id, "avfoundation:0x1234");
        assert_eq!(info.name, "USB Camera");
    }
}

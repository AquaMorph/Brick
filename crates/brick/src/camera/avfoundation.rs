//! Webcams on macOS through AVFoundation (C2).
//!
//! AVFoundation sends the picture but does not let programs set a webcam's
//! exposure time, gain, white balance, or focus. USB Video Class webcams
//! offer them through USB control requests instead ([`uvc`]), so those are
//! used where the webcam has them (CT1). Automatic exposure, white balance,
//! and focus are turned off when the webcam is opened, so the picture does
//! not flicker between frames (C8). Webcams that are not USB Video Class
//! devices, such as built in cameras, only offer the resolution; their
//! automatic settings are locked once they have settled instead.

use std::io::{self, Cursor};
use std::time::Duration;

use brick_avfoundation::{self as avf, Device};
use image::codecs::jpeg::JpegEncoder;

use super::uvc::{self, Uvc};
use super::webcam;
use super::{Backend, Camera, CameraInfo, CameraSettings, Capture, Control, Error, Frame, Result};
use super::{Setting, resolution_label};

const ID_PREFIX: &str = "avfoundation:";
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);
/// Frames dropped after the camera starts for a capture, while the sensor
/// settles.
const WARM_UP_FRAMES: usize = 3;
/// Frames read before judging or locking the exposure, so it has time to
/// settle.
const SETTLE_FRAMES: usize = 15;
/// A mean brightness below this is a black frame.
const BLACK: f64 = 3.0;
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
        Ok(Box::new(Webcam::open(device, unique_id)?))
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

/// A setting offered through a USB Video Class control.
struct WebcamControl {
    setting: Setting,
    control: uvc::Control,
    /// Labels and the control values they stand for, in increasing order.
    choices: Vec<(String, i32)>,
}

struct Webcam {
    device: Device,
    uvc: Option<Uvc>,
    controls: Vec<WebcamControl>,
}

impl Webcam {
    fn open(device: Device, unique_id: &str) -> Result<Self> {
        let mut webcam = Self {
            device,
            uvc: Uvc::open(unique_id),
            controls: Vec::new(),
        };
        webcam.streaming(|webcam| {
            if webcam.uvc.is_some() {
                webcam.fix_automatic();
                webcam.find_controls();
                webcam.check_manual_exposure();
            } else {
                // Without USB Video Class controls, the automatic settings
                // find the scene and are then fixed where the webcam allows
                // it (C8). The webcam is still usable if it does not.
                webcam.brightness();
                let _ = webcam.device.lock_automatic();
            }
        })?;
        Ok(webcam)
    }

    /// Runs `change` while the webcam is streaming, starting it first if it
    /// is not. Some webcams send frames nothing can decode, until they are
    /// plugged in again, after their exposure mode is changed while they are
    /// not streaming, so controls are only ever changed in here.
    fn streaming<T>(&mut self, change: impl FnOnce(&mut Self) -> T) -> Result<T> {
        let was_running = self.device.is_running();
        if !was_running {
            self.device
                .set_running(true)
                .map_err(|e| error(&e, "Could not start the webcam"))?;
            // The webcam is only ready for changes once it sends pictures.
            let _ = self.device.next_frame(FRAME_TIMEOUT);
        }
        let result = change(self);
        if !was_running {
            self.device
                .set_running(false)
                .map_err(|e| error(&e, "Could not stop the webcam"))?;
        }
        Ok(result)
    }

    /// C8: fixed exposure, white balance, and focus. Not every webcam allows
    /// each of these, so failures are ignored. Called while streaming.
    fn fix_automatic(&self) {
        let Some(uvc) = &self.uvc else {
            return;
        };
        if uvc.exposure_modes() & uvc::EXPOSURE_MANUAL != 0 {
            let _ = uvc.set(uvc::Control::AutoExposureMode, uvc::EXPOSURE_MANUAL);
        }
        for control in [
            uvc::Control::AutoExposurePriority,
            uvc::Control::WhiteBalanceAuto,
            uvc::Control::FocusAuto,
        ] {
            if uvc.has(control) {
                let _ = uvc.set(control, 0);
            }
        }
    }

    fn find_controls(&mut self) {
        let Some(uvc) = &self.uvc else {
            return;
        };
        let wanted = [
            (Setting::ShutterSpeed, uvc::Control::ExposureTime),
            (Setting::Gain, uvc::Control::Gain),
            (Setting::WhiteBalance, uvc::Control::WhiteBalanceTemperature),
            (Setting::Focus, uvc::Control::FocusAbsolute),
        ];
        for (setting, control) in wanted {
            if setting == Setting::ShutterSpeed && uvc.exposure_modes() & uvc::EXPOSURE_MANUAL == 0
            {
                continue;
            }
            let Ok(range) = uvc.range(control) else {
                continue;
            };
            let choices = webcam::choices(setting, range);
            if !choices.is_empty() {
                self.controls.push(WebcamControl {
                    setting,
                    control,
                    choices,
                });
            }
        }
    }

    /// Some webcams send only black frames, or none, in manual exposure. If
    /// so, and automatic exposure gives a picture, automatic exposure is kept
    /// and the exposure time is not offered, since the device does not really
    /// allow fixing it (C8). Called while streaming.
    fn check_manual_exposure(&mut self) {
        if !self
            .controls
            .iter()
            .any(|c| c.setting == Setting::ShutterSpeed)
        {
            return;
        }
        if self.brightness().is_some_and(|b| b > BLACK) {
            return;
        }
        let Some(uvc) = &self.uvc else {
            return;
        };
        let modes = uvc.exposure_modes();
        let Some(auto) = [uvc::EXPOSURE_APERTURE_PRIORITY, uvc::EXPOSURE_AUTO]
            .into_iter()
            .find(|mode| modes & mode != 0)
        else {
            return;
        };
        if uvc.set(uvc::Control::AutoExposureMode, auto).is_err() {
            return;
        }
        if self.brightness().is_some_and(|b| b > BLACK) {
            self.controls.retain(|c| c.setting != Setting::ShutterSpeed);
        } else if let Some(uvc) = &self.uvc {
            let _ = uvc.set(uvc::Control::AutoExposureMode, uvc::EXPOSURE_MANUAL);
        }
    }

    /// The mean brightness (0 to 255) of a frame taken once the exposure has
    /// had time to settle, or `None` if the webcam sent no picture. Called
    /// while streaming.
    fn brightness(&mut self) -> Option<f64> {
        let mut last = None;
        for _ in 0..SETTLE_FRAMES {
            match self.device.next_frame(FRAME_TIMEOUT) {
                Ok(image) => last = Some(image),
                Err(_) => break,
            }
        }
        last.map(|image| mean_brightness(&image.rgba))
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
        let resolution = Control {
            setting: Setting::Resolution,
            choices: self
                .device
                .sizes()
                .into_iter()
                .map(|(w, h)| resolution_label(w, h))
                .collect(),
        };
        let others = self.controls.iter().map(|c| Control {
            setting: c.setting,
            choices: c.choices.iter().map(|(label, _)| label.clone()).collect(),
        });
        std::iter::once(resolution).chain(others).collect()
    }

    fn settings(&self) -> CameraSettings {
        let (width, height) = self.device.size();
        let mut settings = CameraSettings::default();
        settings.set(Setting::Resolution, Some(resolution_label(width, height)));
        if let Some(uvc) = &self.uvc {
            for control in &self.controls {
                let Ok(value) = uvc.get(control.control) else {
                    continue;
                };
                let nearest = webcam::nearest(&control.choices, value);
                settings.set(control.setting, nearest.map(str::to_owned));
            }
        }
        settings
    }

    fn set(&mut self, setting: Setting, value: &str) -> Result<()> {
        if setting == Setting::Resolution {
            return self.set_resolution(value);
        }
        let unsupported = || {
            Error::Failed(format!(
                "This webcam does not support {} {value}.",
                setting.label()
            ))
        };
        let (control, raw) = self
            .controls
            .iter()
            .filter(|c| c.setting == setting)
            .find_map(|c| {
                let (_, raw) = c.choices.iter().find(|(label, _)| label == value)?;
                Some((c.control, *raw))
            })
            .ok_or_else(unsupported)?;
        if self.uvc.is_none() {
            return Err(unsupported());
        }
        self.streaming(|webcam| match &webcam.uvc {
            Some(uvc) => uvc.set(control, raw),
            None => Ok(()),
        })?
        .map_err(|e| error(&e, "The webcam refused the setting"))
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

/// The mean of the colour channels of RGBA pixels, from a sample of them;
/// plenty to tell a black frame from a picture.
fn mean_brightness(rgba: &[u8]) -> f64 {
    let (sum, count) = rgba.as_chunks::<4>().0.iter().step_by(61).fold(
        (0u64, 0u64),
        |(sum, count), [r, g, b, _]| {
            (
                sum + u64::from(*r) + u64::from(*g) + u64::from(*b),
                count + 3,
            )
        },
    );
    sum as f64 / count.max(1) as f64
}

fn error(e: &io::Error, context: &str) -> Error {
    if e.kind() == io::ErrorKind::NotConnected {
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

    #[test]
    fn black_frames_are_dark() {
        let black = vec![0u8; 64 * 64 * 4];
        assert_eq!(mean_brightness(&black), 0.0);
        let grey: Vec<u8> = [128, 128, 128, 255].repeat(64 * 64);
        assert_eq!(mean_brightness(&grey), 128.0);
        assert_eq!(mean_brightness(&[]), 0.0);
    }
}

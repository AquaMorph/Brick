//! Webcams on Linux through V4L2 (C2).
//!
//! Webcams do not have a shutter speed, aperture, or ISO. They offer what
//! V4L2 exposes instead: exposure time (shown as a shutter speed), gain,
//! white balance, and focus, where the device has them. Automatic exposure,
//! white balance, and focus are turned off when the webcam is opened, so the
//! picture does not flicker between frames (C8).

use std::io::{self, Cursor};
use std::time::Duration;

use brick_v4l2::{self as v4l2, Device, FourCc, PixFormat, Stream};
use image::codecs::jpeg::JpegEncoder;

use super::webcam::{self, Range};
use super::{Backend, Camera, CameraInfo, CameraSettings, Capture, Control, Error, Frame, Result};
use super::{Setting, resolution_label};

const ID_PREFIX: &str = "v4l2:";
const BUFFERS: u32 = 4;
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);
/// Frames dropped after the stream starts for a capture, while the sensor
/// settles.
const WARM_UP_FRAMES: usize = 3;
const JPEG_QUALITY: u8 = 95;
/// Frames read before judging the exposure, so it has time to settle.
const SETTLE_FRAMES: usize = 12;
/// A mean brightness below this is a black frame.
const BLACK: f64 = 3.0;
/// The `CID_EXPOSURE_AUTO` menu value for aperture priority, the automatic
/// mode UVC webcams support.
const EXPOSURE_AUTO: i32 = 3;
pub struct V4l2Backend;

impl Backend for V4l2Backend {
    fn detect(&mut self) -> Vec<CameraInfo> {
        v4l2::devices().iter().map(camera_info).collect()
    }

    fn open(&mut self, id: &str) -> Result<Box<dyn Camera>> {
        let device = v4l2::devices()
            .into_iter()
            .find(|d| camera_info(d).id == id)
            .ok_or_else(|| Error::Failed("The webcam is not connected.".into()))?;
        let camera = Webcam::open(&device).map_err(|e| error(&e, "Could not open the webcam"))?;
        Ok(Box::new(camera))
    }
}

/// The bus position stays the same when the `/dev/video` number changes, so
/// the webcam is found again after it is plugged back in (C5).
fn camera_info(device: &v4l2::DeviceInfo) -> CameraInfo {
    CameraInfo {
        id: format!("{ID_PREFIX}{}:{}", device.bus_info, device.card),
        name: display_name(&device.card),
    }
}

/// Many drivers repeat the name, as in "Laptop Camera: Laptop Camera".
fn display_name(card: &str) -> String {
    match card.split_once(": ") {
        Some((a, b)) if a == b => a.to_owned(),
        _ => card.to_owned(),
    }
}

struct WebcamControl {
    setting: Setting,
    id: u32,
    /// Labels and the control values they stand for, in increasing order.
    choices: Vec<(String, i32)>,
}

struct Webcam {
    device: Device,
    format: PixFormat,
    /// The frame sizes the webcam offers, largest (native) first (C10).
    sizes: Vec<(u32, u32)>,
    stream: Option<Stream>,
    controls: Vec<WebcamControl>,
}

impl Webcam {
    fn open(info: &v4l2::DeviceInfo) -> io::Result<Self> {
        let device = Device::open(&info.path)?;
        let formats = device.formats()?;
        let fourcc = [FourCc::MJPG, FourCc::YUYV]
            .into_iter()
            .find(|f| formats.contains(f))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "the webcam sends no picture format Brick can read",
                )
            })?;
        let mut sizes = device.frame_sizes(fourcc)?;
        sizes.sort_by_key(|(w, h)| std::cmp::Reverse(u64::from(*w) * u64::from(*h)));
        sizes.dedup();
        let (width, height) = sizes.first().copied().unwrap_or((1280, 720));
        let format = device.set_format(fourcc, width, height)?;
        if format.fourcc != fourcc {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("the webcam would not send {fourcc}"),
            ));
        }

        // C8: fixed exposure, white balance, and focus. Not every webcam
        // allows each of these, so failures are ignored.
        for (id, value) in [
            (v4l2::CID_EXPOSURE_AUTO, v4l2::EXPOSURE_MANUAL),
            (v4l2::CID_EXPOSURE_AUTO_PRIORITY, 0),
            (v4l2::CID_AUTO_WHITE_BALANCE, 0),
            (v4l2::CID_FOCUS_AUTO, 0),
        ] {
            if device.query_control(id).ok().flatten().is_some() {
                let _ = device.set_control(id, value);
            }
        }

        let mut controls = Vec::new();
        let wanted = [
            (Setting::ShutterSpeed, v4l2::CID_EXPOSURE_ABSOLUTE),
            (Setting::Gain, v4l2::CID_GAIN),
            (Setting::WhiteBalance, v4l2::CID_WHITE_BALANCE_TEMPERATURE),
            (Setting::Focus, v4l2::CID_FOCUS_ABSOLUTE),
        ];
        for (setting, id) in wanted {
            let Some(info) = device.query_control(id)? else {
                continue;
            };
            let choices = webcam::choices(
                setting,
                Range {
                    minimum: info.minimum,
                    maximum: info.maximum,
                    step: info.step,
                },
            );
            if !choices.is_empty() {
                controls.push(WebcamControl {
                    setting,
                    id,
                    choices,
                });
            }
        }

        let mut webcam = Self {
            device,
            format,
            sizes,
            stream: None,
            controls,
        };
        webcam.check_manual_exposure();
        Ok(webcam)
    }

    /// Some webcams send only black frames in manual exposure. If so, and
    /// auto exposure gives a picture, auto exposure is kept and the exposure
    /// time is not offered, since the device does not really allow fixing it
    /// (C8).
    fn check_manual_exposure(&mut self) {
        if !self
            .controls
            .iter()
            .any(|c| c.setting == Setting::ShutterSpeed)
        {
            return;
        }
        let Some(manual) = self.brightness() else {
            return;
        };
        if manual > BLACK {
            return;
        }
        if self
            .device
            .set_control(v4l2::CID_EXPOSURE_AUTO, EXPOSURE_AUTO)
            .is_err()
        {
            return;
        }
        if self.brightness().is_some_and(|auto| auto > BLACK) {
            self.controls.retain(|c| c.setting != Setting::ShutterSpeed);
        } else {
            let _ = self
                .device
                .set_control(v4l2::CID_EXPOSURE_AUTO, v4l2::EXPOSURE_MANUAL);
        }
        self.stream = None;
    }

    /// The mean brightness (0 to 255) of a frame taken once the exposure has
    /// had time to settle.
    fn brightness(&mut self) -> Option<f64> {
        let stream = self.start().ok()?;
        let mut data = Vec::new();
        for _ in 0..SETTLE_FRAMES {
            data = stream.next_frame(FRAME_TIMEOUT).ok()?;
        }
        // A small copy is plenty to tell a black frame from a picture.
        let small = self.decode(&data).ok()?.thumbnail(64, 64).into_rgb8();
        let sum: u64 = small.as_raw().iter().map(|v| u64::from(*v)).sum();
        Some(sum as f64 / small.as_raw().len().max(1) as f64)
    }

    fn start(&mut self) -> Result<&mut Stream> {
        if self.stream.is_none() {
            let stream = self
                .device
                .stream(BUFFERS)
                .map_err(|e| error(&e, "Could not start the webcam"))?;
            self.stream = Some(stream);
        }
        self.stream
            .as_mut()
            .ok_or_else(|| Error::Failed("The webcam is not streaming.".into()))
    }

    fn decode(&self, data: &[u8]) -> Result<image::DynamicImage> {
        let unreadable = || Error::Failed("The webcam sent an unreadable frame.".into());
        match self.format.fourcc {
            FourCc::MJPG => image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
                .map_err(|e| Error::Failed(format!("The webcam sent an unreadable frame: {e}"))),
            _ => {
                let rgb = v4l2::yuyv_to_rgb(data, self.format).ok_or_else(unreadable)?;
                image::RgbImage::from_raw(self.format.width, self.format.height, rgb)
                    .map(image::DynamicImage::ImageRgb8)
                    .ok_or_else(unreadable)
            }
        }
    }

    /// Changes the frame size. The stream has to stop while the format
    /// changes, and is restarted if it was running.
    fn set_resolution(&mut self, value: &str) -> Result<()> {
        let (width, height) = self
            .sizes
            .iter()
            .copied()
            .find(|(w, h)| resolution_label(*w, *h) == value)
            .ok_or_else(|| {
                Error::Failed(format!(
                    "This webcam does not support the resolution {value}."
                ))
            })?;
        let streaming = self.stream.take().is_some();
        let format = self
            .device
            .set_format(self.format.fourcc, width, height)
            .map_err(|e| error(&e, "The webcam refused the resolution"))?;
        self.format = format;
        if streaming {
            self.start()?;
        }
        if (format.width, format.height) != (width, height) {
            return Err(Error::Failed(format!(
                "The webcam chose {} instead of {value}.",
                resolution_label(format.width, format.height)
            )));
        }
        Ok(())
    }
}

impl Camera for Webcam {
    fn controls(&self) -> Vec<Control> {
        let resolution = Control {
            setting: Setting::Resolution,
            choices: self
                .sizes
                .iter()
                .map(|(w, h)| resolution_label(*w, *h))
                .collect(),
        };
        let others = self.controls.iter().map(|c| Control {
            setting: c.setting,
            choices: c.choices.iter().map(|(label, _)| label.clone()).collect(),
        });
        std::iter::once(resolution).chain(others).collect()
    }

    fn settings(&self) -> CameraSettings {
        let mut settings = CameraSettings::default();
        settings.set(
            Setting::Resolution,
            Some(resolution_label(self.format.width, self.format.height)),
        );
        for control in &self.controls {
            let Ok(value) = self.device.control(control.id) else {
                continue;
            };
            let nearest = webcam::nearest(&control.choices, value);
            settings.set(control.setting, nearest.map(str::to_owned));
        }
        settings
    }

    fn set(&mut self, setting: Setting, value: &str) -> Result<()> {
        if setting == Setting::Resolution {
            return self.set_resolution(value);
        }
        let (id, raw) = self
            .controls
            .iter()
            .filter(|c| c.setting == setting)
            .find_map(|c| {
                let (_, raw) = c.choices.iter().find(|(label, _)| label == value)?;
                Some((c.id, *raw))
            })
            .ok_or_else(|| {
                Error::Failed(format!(
                    "This webcam does not support {} {value}.",
                    setting.label()
                ))
            })?;
        self.device
            .set_control(id, raw)
            .map_err(|e| error(&e, "The webcam refused the setting"))
    }

    fn set_live_view(&mut self, on: bool) -> Result<()> {
        if on {
            self.start().map(|_| ())
        } else {
            // Stopping the stream turns the webcam's light off.
            self.stream = None;
            Ok(())
        }
    }

    fn live_view(&mut self) -> Result<Frame> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| Error::Failed("Live view is off.".into()))?;
        // Older queued frames are dropped so live view never lags (CT2).
        let data = stream
            .latest_frame(FRAME_TIMEOUT)
            .map_err(|e| error(&e, "No live view frame"))?;
        let rgba = self.decode(&data)?.into_rgba8();
        Ok(Frame {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    }

    /// Webcams have no shutter: a capture is the next frame from the stream.
    fn capture(&mut self) -> Result<Capture> {
        let was_streaming = self.stream.is_some();
        let stream = self.start()?;
        let read = |stream: &mut Stream| {
            stream
                .next_frame(FRAME_TIMEOUT)
                .map_err(|e| error(&e, "The webcam sent no picture"))
        };
        if was_streaming {
            // Frames already waiting were taken before the capture was asked
            // for, possibly with the animator's hand still in the shot.
            while stream
                .ready()
                .map_err(|e| error(&e, "The webcam stopped"))?
            {
                read(stream)?;
            }
        } else {
            for _ in 0..WARM_UP_FRAMES {
                read(stream)?;
            }
        }
        let data = read(stream)?;
        if !was_streaming {
            self.stream = None;
        }

        let rgb = self.decode(&data)?.into_rgb8();
        // A motion JPEG frame is a complete JPEG when it carries its Huffman
        // tables, so it can be kept as the camera sent it.
        let jpeg = if self.format.fourcc == FourCc::MJPG && has_huffman_tables(&data) {
            data
        } else {
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), JPEG_QUALITY)
                .encode_image(&rgb)
                .map_err(|e| Error::Failed(format!("Could not encode the image: {e}")))?;
            jpeg
        };
        Ok(Capture { jpeg, raw: None })
    }
}

fn error(e: &io::Error, context: &str) -> Error {
    if v4l2::is_disconnected(e) {
        Error::Disconnected("The webcam was disconnected.".into())
    } else {
        Error::Failed(format!("{context}: {e}"))
    }
}

fn has_huffman_tables(jpeg: &[u8]) -> bool {
    jpeg.windows(2).any(|w| w == [0xff, 0xc4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_not_repeated() {
        assert_eq!(
            display_name("Laptop Camera: Laptop Camera"),
            "Laptop Camera"
        );
        assert_eq!(display_name("C920: HD Pro"), "C920: HD Pro");
    }
}

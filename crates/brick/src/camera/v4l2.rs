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

use super::Setting;
use super::{Backend, Camera, CameraInfo, CameraSettings, Capture, Control, Error, Frame, Result};

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
/// Most values offered for a control with a wide range.
const MAX_STEPS: i32 = 20;

/// Shutter speeds offered for the exposure time, with their length in
/// seconds. Only those within the webcam's range are offered.
const SHUTTER_SPEEDS: [(&str, f64); 13] = [
    ("1/4000", 1.0 / 4000.0),
    ("1/2000", 1.0 / 2000.0),
    ("1/1000", 1.0 / 1000.0),
    ("1/500", 1.0 / 500.0),
    ("1/250", 1.0 / 250.0),
    ("1/125", 1.0 / 125.0),
    ("1/60", 1.0 / 60.0),
    ("1/30", 1.0 / 30.0),
    ("1/15", 1.0 / 15.0),
    ("1/8", 1.0 / 8.0),
    ("1/4", 1.0 / 4.0),
    ("1/2", 1.0 / 2.0),
    ("1\"", 1.0),
];

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
        let (width, height) = device
            .frame_sizes(fourcc)?
            .into_iter()
            .max_by_key(|(w, h)| u64::from(*w) * u64::from(*h))
            .unwrap_or((1280, 720));
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
            let choices = match setting {
                Setting::ShutterSpeed => exposure_choices(&info),
                Setting::WhiteBalance => kelvin_choices(&info),
                _ => steps(&info, MAX_STEPS)
                    .into_iter()
                    .map(|v| (v.to_string(), v))
                    .collect(),
            };
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
        let rgb = self.decode(&data).ok()?;
        let sum: u64 = rgb.as_raw().iter().map(|v| u64::from(*v)).sum();
        Some(sum as f64 / rgb.as_raw().len().max(1) as f64)
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

    fn decode(&self, data: &[u8]) -> Result<image::RgbImage> {
        match self.format.fourcc {
            FourCc::MJPG => image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
                .map(|image| image.to_rgb8())
                .map_err(|e| Error::Failed(format!("The webcam sent an unreadable frame: {e}"))),
            _ => yuyv_to_rgb(data, self.format),
        }
    }
}

impl Camera for Webcam {
    fn controls(&self) -> Vec<Control> {
        self.controls
            .iter()
            .map(|c| Control {
                setting: c.setting,
                choices: c.choices.iter().map(|(label, _)| label.clone()).collect(),
            })
            .collect()
    }

    fn settings(&self) -> CameraSettings {
        let mut settings = CameraSettings::default();
        for control in &self.controls {
            let Ok(value) = self.device.control(control.id) else {
                continue;
            };
            let nearest = control
                .choices
                .iter()
                .min_by_key(|(_, v)| (i64::from(*v) - i64::from(value)).abs());
            settings.set(control.setting, nearest.map(|(label, _)| label.clone()));
        }
        settings
    }

    fn set(&mut self, setting: Setting, value: &str) -> Result<()> {
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
        let data = stream
            .next_frame(FRAME_TIMEOUT)
            .map_err(|e| error(&e, "No live view frame"))?;
        let rgb = self.decode(&data)?;
        let (width, height) = rgb.dimensions();
        let rgba = rgb.pixels().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        Ok(Frame {
            width,
            height,
            rgba,
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

        let rgb = self.decode(&data)?;
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

/// Exposure time is in units of 100 µs.
fn exposure_choices(info: &v4l2::ControlInfo) -> Vec<(String, i32)> {
    let mut choices: Vec<(String, i32)> = SHUTTER_SPEEDS
        .iter()
        .filter_map(|(label, seconds)| {
            let units = snap(info, (seconds * 10_000.0).round() as i32);
            let exact = (f64::from(units) / 10_000.0 - seconds).abs() / seconds < 0.05;
            exact.then(|| ((*label).to_owned(), units))
        })
        .collect();
    choices.dedup_by_key(|(_, v)| *v);
    choices
}

fn kelvin_choices(info: &v4l2::ControlInfo) -> Vec<(String, i32)> {
    let mut choices: Vec<(String, i32)> = (info.minimum..=info.maximum)
        .step_by(100)
        .map(|k| snap(info, (k + 99) / 100 * 100))
        .map(|k| (format!("{k} K"), k))
        .collect();
    choices.dedup_by_key(|(_, v)| *v);
    choices
}

/// Up to `count + 1` evenly spaced values across a control's range.
fn steps(info: &v4l2::ControlInfo, count: i32) -> Vec<i32> {
    let range = i64::from(info.maximum) - i64::from(info.minimum);
    if range <= 0 {
        return vec![info.minimum];
    }
    let mut values: Vec<i32> = (0..=i64::from(count))
        .map(|i| {
            snap(
                info,
                (i64::from(info.minimum) + range * i / i64::from(count)) as i32,
            )
        })
        .collect();
    values.dedup();
    values
}

/// The closest value to `value` the control accepts.
fn snap(info: &v4l2::ControlInfo, value: i32) -> i32 {
    let step = i64::from(info.step.max(1));
    let min = i64::from(info.minimum);
    let offset = (i64::from(value) - min + step / 2).div_euclid(step) * step;
    (min + offset).clamp(min, i64::from(info.maximum)) as i32
}

fn has_huffman_tables(jpeg: &[u8]) -> bool {
    jpeg.windows(2).any(|w| w == [0xff, 0xc4])
}

/// Converts packed YUYV 4:2:2 (BT.601) to RGB.
fn yuyv_to_rgb(data: &[u8], format: PixFormat) -> Result<image::RgbImage> {
    let (width, height) = (format.width as usize, format.height as usize);
    let stride = (format.bytes_per_line as usize).max(width * 2);
    if data.len() < stride * (height - 1) + width * 2 {
        return Err(Error::Failed("The webcam sent a short frame.".into()));
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in data.chunks(stride).take(height) {
        for pair in row[..width * 2].chunks_exact(4) {
            let (y0, u, y1, v) = (pair[0], pair[1], pair[2], pair[3]);
            rgb.extend_from_slice(&yuv(y0, u, v));
            rgb.extend_from_slice(&yuv(y1, u, v));
        }
    }
    image::RgbImage::from_raw(format.width, format.height, rgb)
        .ok_or_else(|| Error::Failed("The webcam sent a short frame.".into()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn control(minimum: i32, maximum: i32, step: i32) -> v4l2::ControlInfo {
        v4l2::ControlInfo {
            id: 0,
            minimum,
            maximum,
            step,
            default: minimum,
            inactive: false,
        }
    }

    #[test]
    fn exposure_offers_shutter_speeds_in_range() {
        // 1 ms to 62.5 ms, as on a common laptop webcam.
        let labels: Vec<String> = exposure_choices(&control(10, 625, 1))
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        assert_eq!(
            labels,
            ["1/1000", "1/500", "1/250", "1/125", "1/60", "1/30"]
        );
    }

    #[test]
    fn steps_follow_the_control_step() {
        assert_eq!(steps(&control(0, 4, 1), 20), [0, 1, 2, 3, 4]);
        let wide = steps(&control(0, 255, 5), 20);
        assert_eq!(wide.first(), Some(&0));
        assert_eq!(wide.last(), Some(&255));
        assert!(wide.iter().all(|v| v % 5 == 0));
        assert_eq!(snap(&control(2800, 6500, 10), 4604), 4600);
    }

    #[test]
    fn white_balance_in_hundreds_of_kelvin() {
        let choices = kelvin_choices(&control(2800, 6500, 10));
        assert_eq!(choices.first().map(|c| c.0.as_str()), Some("2800 K"));
        assert_eq!(choices.last().map(|c| c.0.as_str()), Some("6500 K"));
        assert_eq!(choices.len(), 38);
    }

    #[test]
    fn names_are_not_repeated() {
        assert_eq!(
            display_name("Laptop Camera: Laptop Camera"),
            "Laptop Camera"
        );
        assert_eq!(display_name("C920: HD Pro"), "C920: HD Pro");
    }

    #[test]
    fn yuyv_grey_stays_grey() {
        let format = PixFormat {
            width: 2,
            height: 1,
            fourcc: FourCc::YUYV,
            bytes_per_line: 4,
        };
        let rgb = yuyv_to_rgb(&[126, 128, 126, 128], format).unwrap();
        assert_eq!(rgb.get_pixel(0, 0).0, [128, 128, 128]);
    }
}

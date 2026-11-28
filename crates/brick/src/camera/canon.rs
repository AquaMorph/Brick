//! Canon DSLRs through the EDSDK (C3).
//!
//! The camera's shutter speed, aperture, and ISO are offered as the camera
//! lists them, which depends on the mode dial and the lens: in Manual all
//! three can be set. The resolution is the size of the JPEG the camera saves
//! (C10). Whether it also saves a RAW file is left as it is set on the camera
//! (C4), but a camera set to save no JPEG at all is switched to one when it is
//! opened, since every capture needs a JPEG to show (P14).

use std::thread;
use std::time::{Duration, Instant};

use brick_edsdk::{self as edsdk, Property};

use super::Setting;
use super::{Backend, Camera, CameraInfo, CameraSettings, Capture, Control, Error, Frame, Result};

const ID_PREFIX: &str = "canon:";
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);
/// The camera sends live view at about 30 frames a second, so asking more
/// often only keeps it busy.
const LIVE_INTERVAL: Duration = Duration::from_millis(33);
const FRAME_RETRY: Duration = Duration::from_millis(10);
/// Long enough for a 30 second exposure and the transfer after it.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct CanonBackend;

impl Backend for CanonBackend {
    fn detect(&mut self) -> Vec<CameraInfo> {
        let devices = edsdk::cameras().unwrap_or_default();
        camera_infos(&devices)
            .into_iter()
            .map(|(info, _)| info)
            .collect()
    }

    fn open(&mut self, id: &str) -> Result<Box<dyn Camera>> {
        let devices = edsdk::cameras().map_err(error)?;
        let port = camera_infos(&devices)
            .into_iter()
            .find(|(info, _)| info.id == id)
            .map(|(_, port)| port)
            .ok_or_else(|| Error::Failed("The camera is not connected.".into()))?;
        let camera = edsdk::Camera::open(&port).map_err(error)?;
        Ok(Box::new(Dslr::open(camera)?))
    }
}

/// Detected cameras with the port each is connected to. The port changes when
/// a camera is plugged in again, so cameras are known by their model name
/// instead (C5), numbered when several of the same model are connected.
fn camera_infos(devices: &[edsdk::DeviceInfo]) -> Vec<(CameraInfo, String)> {
    let mut infos: Vec<(CameraInfo, String)> = Vec::new();
    for device in devices {
        let same = infos
            .iter()
            .filter(|(info, _)| info.name.starts_with(&device.description))
            .count();
        let name = if same == 0 {
            device.description.clone()
        } else {
            format!("{} ({})", device.description, same + 1)
        };
        let info = CameraInfo {
            id: format!("{ID_PREFIX}{name}"),
            name,
        };
        infos.push((info, device.port.clone()));
    }
    infos
}

/// A setting backed by a camera property, with the values the camera offered
/// when it was opened.
struct DslrControl {
    setting: Setting,
    property: Property,
    choices: Vec<(String, u32)>,
}

struct Dslr {
    camera: edsdk::Camera,
    controls: Vec<DslrControl>,
    /// Every image quality the camera offers.
    qualities: Vec<u32>,
    live_view: bool,
    last_frame: Instant,
}

impl Dslr {
    fn open(camera: edsdk::Camera) -> Result<Self> {
        let qualities = camera.choices(Property::ImageQuality).unwrap_or_default();
        let quality = camera.get(Property::ImageQuality).map_err(error)?;
        if jpeg(quality).is_none()
            && let Some(with_jpeg) = with_jpeg(quality, &qualities)
        {
            camera
                .set(Property::ImageQuality, with_jpeg)
                .map_err(error)?;
        }

        let mut controls = Vec::new();
        for (setting, property) in [
            (Setting::ShutterSpeed, Property::Tv),
            (Setting::Aperture, Property::Av),
            (Setting::Iso, Property::Iso),
        ] {
            let codes = camera.choices(property).unwrap_or_default();
            let choices = labelled(setting, &codes);
            if !choices.is_empty() {
                controls.push(DslrControl {
                    setting,
                    property,
                    choices,
                });
            }
        }
        Ok(Self {
            camera,
            controls,
            qualities,
            live_view: false,
            last_frame: Instant::now(),
        })
    }

    fn quality(&self) -> Result<u32> {
        self.camera.get(Property::ImageQuality).map_err(error)
    }
}

impl Camera for Dslr {
    fn controls(&self) -> Vec<Control> {
        let mut controls = Vec::new();
        if let Ok(quality) = self.quality() {
            let resolutions = resolutions(quality, &self.qualities);
            if !resolutions.is_empty() {
                controls.push(Control {
                    setting: Setting::Resolution,
                    choices: resolutions.into_iter().map(|(label, _)| label).collect(),
                });
            }
        }
        controls.extend(self.controls.iter().map(|c| Control {
            setting: c.setting,
            choices: c.choices.iter().map(|(label, _)| label.clone()).collect(),
        }));
        controls
    }

    fn settings(&self) -> CameraSettings {
        let mut settings = CameraSettings::default();
        if let Ok(quality) = self.quality() {
            let size = jpeg(quality).and_then(|image| size_label(image.size));
            settings.set(Setting::Resolution, size.map(str::to_owned));
        }
        for control in &self.controls {
            let value = self.camera.get(control.property).ok();
            let label = value.and_then(|code| label(control.setting, code));
            settings.set(control.setting, label.map(str::to_owned));
        }
        settings
    }

    fn set(&mut self, setting: Setting, value: &str) -> Result<()> {
        let unsupported = || {
            Error::Failed(format!(
                "This camera does not offer {} {value}.",
                setting.label()
            ))
        };
        if setting == Setting::Resolution {
            let quality = self.quality()?;
            let code = resolutions(quality, &self.qualities)
                .into_iter()
                .find(|(label, _)| label == value)
                .map(|(_, code)| code)
                .ok_or_else(unsupported)?;
            return self.camera.set(Property::ImageQuality, code).map_err(error);
        }
        let (property, code) = self
            .controls
            .iter()
            .filter(|c| c.setting == setting)
            .find_map(|c| {
                let (_, code) = c.choices.iter().find(|(label, _)| label == value)?;
                Some((c.property, *code))
            })
            .ok_or_else(unsupported)?;
        self.camera.set(property, code).map_err(error)
    }

    fn set_live_view(&mut self, on: bool) -> Result<()> {
        self.camera.set_live_view(on).map_err(error)?;
        self.live_view = on;
        Ok(())
    }

    fn live_view(&mut self) -> Result<Frame> {
        if !self.live_view {
            return Err(Error::Failed("Live view is off.".into()));
        }
        thread::sleep(LIVE_INTERVAL.saturating_sub(self.last_frame.elapsed()));
        let deadline = Instant::now() + FRAME_TIMEOUT;
        let jpeg = loop {
            if let Some(jpeg) = self.camera.live_view_frame().map_err(error)? {
                break jpeg;
            }
            if Instant::now() >= deadline {
                return Err(Error::Failed("No live view frame.".into()));
            }
            thread::sleep(FRAME_RETRY);
        };
        self.last_frame = Instant::now();
        let image = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
            .map_err(|e| Error::Failed(format!("Unreadable live view frame: {e}")))?
            .into_rgba8();
        Ok(Frame {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        })
    }

    fn capture(&mut self) -> Result<Capture> {
        let quality = self.quality()?;
        if jpeg(quality).is_none() {
            return Err(Error::Failed(
                "The camera is set to save no JPEG. Set its image quality to JPEG or RAW+JPEG."
                    .into(),
            ));
        }
        let count = images(quality).iter().flatten().count();
        let files = self.camera.capture(count, CAPTURE_TIMEOUT).map_err(error)?;
        split_capture(files)
    }
}

/// Sorts the files of a capture into the JPEG and the RAW file (C4).
fn split_capture(files: Vec<edsdk::File>) -> Result<Capture> {
    let mut jpeg = None;
    let mut raw = None;
    for file in files {
        let extension = file
            .name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        if extension == "jpg" || extension == "jpeg" {
            jpeg = Some(file.data);
        } else if !extension.is_empty() {
            raw = Some((extension, file.data));
        }
    }
    let jpeg = jpeg.ok_or_else(|| Error::Failed("The camera sent no JPEG.".into()))?;
    Ok(Capture { jpeg, raw })
}

fn error(e: edsdk::Error) -> Error {
    if e.is_disconnected() {
        Error::Disconnected("The camera was disconnected.".into())
    } else {
        Error::Failed(format!("{e}."))
    }
}

/// The camera's values for a setting with their labels, in the camera's
/// order. Codes without a label are left out, and so are codes that repeat a
/// label, such as the half stop and third stop values that are both 1/10.
fn labelled(setting: Setting, codes: &[u32]) -> Vec<(String, u32)> {
    let mut choices: Vec<(String, u32)> = Vec::new();
    for code in codes {
        if let Some(label) = label(setting, *code)
            && !choices.iter().any(|(l, _)| l == label)
        {
            choices.push((label.to_owned(), *code));
        }
    }
    choices
}

fn label(setting: Setting, code: u32) -> Option<&'static str> {
    let table: &[(u32, &str)] = match setting {
        Setting::ShutterSpeed => &SHUTTER_SPEEDS,
        Setting::Aperture => &APERTURES,
        Setting::Iso => &ISOS,
        Setting::Resolution | Setting::Gain | Setting::WhiteBalance | Setting::Focus => &[],
    };
    table.iter().find(|(c, _)| *c == code).map(|(_, l)| *l)
}

/// One image saved by a capture, as encoded in an image quality code: the
/// image size, the file format, and the compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Image {
    size: u32,
    format: u32,
    compression: u32,
}

const FORMAT_JPEG: u32 = 1;
const FORMATS_RAW: [u32; 3] = [2, 4, 6];

/// The one or two images an image quality code saves. The high 16 bits
/// describe the first image and the low 16 bits the second, which has format
/// 0 when there is none.
fn images(quality: u32) -> [Option<Image>; 2] {
    let image = |bits: u32| {
        let image = Image {
            size: bits >> 8,
            format: (bits >> 4) & 0xF,
            compression: bits & 0xF,
        };
        (image.format != 0).then_some(image)
    };
    [image(quality >> 16), image(quality & 0xFFFF)]
}

fn jpeg(quality: u32) -> Option<Image> {
    images(quality)
        .into_iter()
        .flatten()
        .find(|image| image.format == FORMAT_JPEG)
}

/// The RAW image an image quality code saves, if any.
fn raw(quality: u32) -> Option<Image> {
    images(quality)
        .into_iter()
        .flatten()
        .find(|image| FORMATS_RAW.contains(&image.format))
}

fn size_label(size: u32) -> Option<&'static str> {
    Some(match size {
        0 => "Large",
        1 => "Medium",
        2 => "Small",
        5 => "Medium 1",
        6 => "Medium 2",
        14 => "Small 1",
        15 => "Small 2",
        16 => "Small 3",
        _ => return None,
    })
}

/// The JPEG sizes the camera can save alongside the same RAW file as
/// `quality`, with the image quality for each, in the camera's order. The
/// best compression of each size is used.
fn resolutions(quality: u32, available: &[u32]) -> Vec<(String, u32)> {
    let mut resolutions: Vec<(String, u32)> = Vec::new();
    for code in available {
        if raw(*code) != raw(quality) {
            continue;
        }
        let Some(image) = jpeg(*code) else {
            continue;
        };
        let Some(label) = size_label(image.size) else {
            continue;
        };
        match resolutions.iter_mut().find(|(l, _)| l == label) {
            Some(existing) => {
                let better = jpeg(existing.1).is_none_or(|e| image.compression > e.compression);
                if better {
                    existing.1 = *code;
                }
            }
            None => resolutions.push((label.to_owned(), *code)),
        }
    }
    resolutions
}

/// The image quality to switch to when `quality` saves no JPEG: the largest,
/// finest JPEG, with the same RAW file when the camera allows it.
fn with_jpeg(quality: u32, available: &[u32]) -> Option<u32> {
    available
        .iter()
        .filter_map(|code| Some((*code, jpeg(*code)?)))
        .max_by_key(|(code, image)| {
            (
                raw(*code) == raw(quality),
                image.size == 0,
                image.compression,
            )
        })
        .map(|(code, _)| code)
}

/// Shutter speed codes. Each stop is 8, with thirds at 3 and 5 and half stops
/// at 4.
const SHUTTER_SPEEDS: [(u32, &str); 75] = [
    (0x0C, "Bulb"),
    (0x10, "30\""),
    (0x13, "25\""),
    (0x14, "20\""),
    (0x15, "20\""),
    (0x18, "15\""),
    (0x1B, "13\""),
    (0x1C, "10\""),
    (0x1D, "10\""),
    (0x20, "8\""),
    (0x23, "6\""),
    (0x24, "6\""),
    (0x25, "5\""),
    (0x28, "4\""),
    (0x2B, "3\"2"),
    (0x2C, "3\""),
    (0x2D, "2\"5"),
    (0x30, "2\""),
    (0x33, "1\"6"),
    (0x34, "1\"5"),
    (0x35, "1\"3"),
    (0x38, "1\""),
    (0x3B, "0\"8"),
    (0x3C, "0\"7"),
    (0x3D, "0\"6"),
    (0x40, "0\"5"),
    (0x43, "0\"4"),
    (0x44, "0\"3"),
    (0x45, "0\"3"),
    (0x48, "1/4"),
    (0x4B, "1/5"),
    (0x4C, "1/6"),
    (0x4D, "1/6"),
    (0x50, "1/8"),
    (0x53, "1/10"),
    (0x54, "1/10"),
    (0x55, "1/13"),
    (0x58, "1/15"),
    (0x5B, "1/20"),
    (0x5C, "1/20"),
    (0x5D, "1/25"),
    (0x60, "1/30"),
    (0x63, "1/40"),
    (0x64, "1/45"),
    (0x65, "1/50"),
    (0x68, "1/60"),
    (0x6B, "1/80"),
    (0x6C, "1/90"),
    (0x6D, "1/100"),
    (0x70, "1/125"),
    (0x73, "1/160"),
    (0x74, "1/180"),
    (0x75, "1/200"),
    (0x78, "1/250"),
    (0x7B, "1/320"),
    (0x7C, "1/350"),
    (0x7D, "1/400"),
    (0x80, "1/500"),
    (0x83, "1/640"),
    (0x84, "1/750"),
    (0x85, "1/800"),
    (0x88, "1/1000"),
    (0x8B, "1/1250"),
    (0x8C, "1/1500"),
    (0x8D, "1/1600"),
    (0x90, "1/2000"),
    (0x93, "1/2500"),
    (0x94, "1/3000"),
    (0x95, "1/3200"),
    (0x98, "1/4000"),
    (0x9B, "1/5000"),
    (0x9C, "1/6000"),
    (0x9D, "1/6400"),
    (0xA0, "1/8000"),
    (0xA3, "1/10000"),
];

/// Aperture codes, laid out like the shutter speeds.
const APERTURES: [(u32, &str); 53] = [
    (0x08, "f/1"),
    (0x0B, "f/1.1"),
    (0x0C, "f/1.2"),
    (0x0D, "f/1.2"),
    (0x10, "f/1.4"),
    (0x13, "f/1.6"),
    (0x14, "f/1.8"),
    (0x15, "f/1.8"),
    (0x18, "f/2"),
    (0x1B, "f/2.2"),
    (0x1C, "f/2.5"),
    (0x1D, "f/2.5"),
    (0x20, "f/2.8"),
    (0x23, "f/3.2"),
    (0x24, "f/3.5"),
    (0x25, "f/3.5"),
    (0x28, "f/4"),
    (0x2B, "f/4.5"),
    (0x2C, "f/4.5"),
    (0x2D, "f/5"),
    (0x30, "f/5.6"),
    (0x33, "f/6.3"),
    (0x34, "f/6.7"),
    (0x35, "f/7.1"),
    (0x38, "f/8"),
    (0x3B, "f/9"),
    (0x3C, "f/9.5"),
    (0x3D, "f/10"),
    (0x40, "f/11"),
    (0x43, "f/13"),
    (0x44, "f/13"),
    (0x45, "f/14"),
    (0x48, "f/16"),
    (0x4B, "f/18"),
    (0x4C, "f/19"),
    (0x4D, "f/20"),
    (0x50, "f/22"),
    (0x53, "f/25"),
    (0x54, "f/27"),
    (0x55, "f/29"),
    (0x58, "f/32"),
    (0x5B, "f/36"),
    (0x5C, "f/38"),
    (0x5D, "f/40"),
    (0x60, "f/45"),
    (0x63, "f/51"),
    (0x64, "f/54"),
    (0x65, "f/57"),
    (0x68, "f/64"),
    (0x6B, "f/72"),
    (0x6C, "f/76"),
    (0x6D, "f/80"),
    (0x70, "f/91"),
];

/// ISO codes in thirds of a stop.
const ISOS: [(u32, &str); 38] = [
    (0x00, "Auto"),
    (0x28, "6"),
    (0x30, "12"),
    (0x38, "25"),
    (0x40, "50"),
    (0x48, "100"),
    (0x4B, "125"),
    (0x4D, "160"),
    (0x50, "200"),
    (0x53, "250"),
    (0x55, "320"),
    (0x58, "400"),
    (0x5B, "500"),
    (0x5D, "640"),
    (0x60, "800"),
    (0x63, "1000"),
    (0x65, "1250"),
    (0x68, "1600"),
    (0x6B, "2000"),
    (0x6D, "2500"),
    (0x70, "3200"),
    (0x73, "4000"),
    (0x75, "5000"),
    (0x78, "6400"),
    (0x7B, "8000"),
    (0x7D, "10000"),
    (0x80, "12800"),
    (0x83, "16000"),
    (0x85, "20000"),
    (0x88, "25600"),
    (0x8B, "32000"),
    (0x8D, "40000"),
    (0x90, "51200"),
    (0x93, "64000"),
    (0x95, "80000"),
    (0x98, "102400"),
    (0xA0, "204800"),
    (0xA8, "409600"),
];

#[cfg(test)]
mod tests {
    use super::super::exposure;
    use super::*;

    /// The exposure a code stands for in thirds of a stop from `zero`, or
    /// `None` for half stops.
    fn code_thirds(code: u32, zero: u32) -> Option<i32> {
        let steps = i32::try_from(code).ok()? - i32::try_from(zero).ok()?;
        let third = match steps.rem_euclid(8) {
            0 => 0,
            3 => 1,
            5 => 2,
            _ => return None,
        };
        Some(steps.div_euclid(8) * 3 + third)
    }

    /// Every label reads back as the exposure its code stands for, so the
    /// exposure rulers line up with the camera (CT9).
    #[test]
    fn labels_match_their_codes() {
        type Table<'a> = (Setting, &'a [(u32, &'a str)], u32, i32);
        let tables: [Table; 3] = [
            (Setting::ShutterSpeed, &SHUTTER_SPEEDS, 0x38, -1),
            (Setting::Aperture, &APERTURES, 0x08, -1),
            (Setting::Iso, &ISOS, 0x48, 1),
        ];
        for (setting, table, zero, sign) in tables {
            for (code, label) in table {
                let Some(expected) = code_thirds(*code, zero) else {
                    continue;
                };
                let read = exposure::thirds(setting, label);
                if *label == "Bulb" || *label == "Auto" {
                    assert_eq!(read, None);
                } else {
                    assert_eq!(read, Some(sign * expected), "{label} ({code:#x})");
                }
            }
        }
    }

    #[test]
    fn repeated_labels_are_offered_once() {
        let choices = labelled(Setting::ShutterSpeed, &[0x50, 0x53, 0x54, 0x58, 0x01]);
        let labels: Vec<&str> = choices.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["1/8", "1/10", "1/15"]);
        assert_eq!(choices[1].1, 0x53);
    }

    const LARGE_FINE: u32 = 0x0013_FF0F;
    const LARGE_NORMAL: u32 = 0x0012_FF0F;
    const MEDIUM_FINE: u32 = 0x0113_FF0F;
    const SMALL_1_FINE: u32 = 0x0E13_FF0F;
    const RAW: u32 = 0x0064_FF0F;
    const RAW_LARGE_FINE: u32 = 0x0064_0013;
    const RAW_MEDIUM_FINE: u32 = 0x0064_0113;
    const CRAW_LARGE_FINE: u32 = 0x0063_0013;
    const HEIF_LARGE: u32 = 0x0080_FF0F;

    #[test]
    fn image_quality_names_its_images() {
        assert_eq!(images(LARGE_FINE).iter().flatten().count(), 1);
        assert_eq!(images(RAW_LARGE_FINE).iter().flatten().count(), 2);
        assert_eq!(jpeg(RAW_MEDIUM_FINE).map(|i| i.size), Some(1));
        assert!(raw(RAW_MEDIUM_FINE).is_some());
        assert!(jpeg(RAW).is_none());
        assert!(jpeg(HEIF_LARGE).is_none());
        assert!(raw(LARGE_FINE).is_none());
        assert_ne!(raw(RAW_LARGE_FINE), raw(CRAW_LARGE_FINE));
    }

    #[test]
    fn resolutions_keep_the_raw_setting() {
        let available = [
            LARGE_NORMAL,
            LARGE_FINE,
            MEDIUM_FINE,
            SMALL_1_FINE,
            RAW,
            RAW_LARGE_FINE,
            RAW_MEDIUM_FINE,
            CRAW_LARGE_FINE,
        ];
        assert_eq!(
            resolutions(LARGE_NORMAL, &available),
            [
                ("Large".to_owned(), LARGE_FINE),
                ("Medium".to_owned(), MEDIUM_FINE),
                ("Small 1".to_owned(), SMALL_1_FINE),
            ]
        );
        assert_eq!(
            resolutions(RAW_LARGE_FINE, &available),
            [
                ("Large".to_owned(), RAW_LARGE_FINE),
                ("Medium".to_owned(), RAW_MEDIUM_FINE),
            ]
        );
    }

    #[test]
    fn cameras_saving_no_jpeg_get_one() {
        let available = [
            LARGE_FINE,
            MEDIUM_FINE,
            RAW,
            RAW_MEDIUM_FINE,
            RAW_LARGE_FINE,
        ];
        assert_eq!(with_jpeg(RAW, &available), Some(RAW_LARGE_FINE));
        assert_eq!(with_jpeg(HEIF_LARGE, &available), Some(LARGE_FINE));
        assert_eq!(with_jpeg(RAW, &[RAW, HEIF_LARGE]), None);
    }

    #[test]
    fn captures_split_into_jpeg_and_raw() {
        let file = |name: &str, data: u8| edsdk::File {
            name: name.into(),
            data: vec![data],
        };
        let capture =
            split_capture(vec![file("IMG_0001.CR3", 1), file("IMG_0001.JPG", 2)]).unwrap();
        assert_eq!(capture.jpeg, [2]);
        assert_eq!(capture.raw, Some(("cr3".into(), vec![1])));
        assert!(split_capture(vec![file("IMG_0001.CR3", 1)]).is_err());
    }

    #[test]
    fn cameras_of_one_model_are_numbered() {
        let device = |port: &str| edsdk::DeviceInfo {
            port: port.into(),
            description: "Canon EOS R6".into(),
        };
        let infos = camera_infos(&[device("usb:1"), device("usb:2")]);
        assert_eq!(infos[0].0.id, "canon:Canon EOS R6");
        assert_eq!(infos[1].0.name, "Canon EOS R6 (2)");
        assert_eq!(infos[1].1, "usb:2");
    }
}

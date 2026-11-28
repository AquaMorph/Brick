//! A simulated camera for development builds, so the program can be built
//! and tested without hardware (C9).
//!
//! It draws a small tabletop scene with a moving figure and a focus target.
//! Shutter speed, aperture, and ISO change the exposure the way they would
//! on a real camera, so the settings have a visible effect.

use std::io::Cursor;
use std::thread;
use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;

use super::{Backend, Camera, CameraInfo, CameraSettings, Capture, Control, Error, Frame, Result};
use super::{Setting, resolution_label};

const ID: &str = "simulated";
const LIVE_SIZE: (u32, u32) = (960, 640);
/// Capture sizes, native first (C10).
const RESOLUTIONS: [(u32, u32); 3] = [(2400, 1600), (1800, 1200), (1200, 800)];
const LIVE_INTERVAL: Duration = Duration::from_millis(33);
/// How long a capture takes, like the shutter and transfer of a real camera.
const CAPTURE_TIME: Duration = Duration::from_millis(400);

/// Shutter speeds and their exposure time in seconds.
const SHUTTER_SPEEDS: [(&str, f32); 11] = [
    ("1\"", 1.0),
    ("1/2", 1.0 / 2.0),
    ("1/4", 1.0 / 4.0),
    ("1/8", 1.0 / 8.0),
    ("1/15", 1.0 / 15.0),
    ("1/30", 1.0 / 30.0),
    ("1/60", 1.0 / 60.0),
    ("1/125", 1.0 / 125.0),
    ("1/250", 1.0 / 250.0),
    ("1/500", 1.0 / 500.0),
    ("1/1000", 1.0 / 1000.0),
];
const APERTURES: [(&str, f32); 8] = [
    ("f/1.8", 1.8),
    ("f/2.8", 2.8),
    ("f/4", 4.0),
    ("f/5.6", 5.6),
    ("f/8", 8.0),
    ("f/11", 11.0),
    ("f/16", 16.0),
    ("f/22", 22.0),
];
const ISOS: [(&str, f32); 7] = [
    ("100", 100.0),
    ("200", 200.0),
    ("400", 400.0),
    ("800", 800.0),
    ("1600", 1600.0),
    ("3200", 3200.0),
    ("6400", 6400.0),
];

pub struct SimulatedBackend;

impl Backend for SimulatedBackend {
    fn detect(&mut self) -> Vec<CameraInfo> {
        vec![CameraInfo {
            id: ID.into(),
            name: "Simulated camera".into(),
        }]
    }

    fn open(&mut self, id: &str) -> Result<Box<dyn Camera>> {
        if id != ID {
            return Err(Error::Failed(format!("No simulated camera called {id}.")));
        }
        Ok(Box::new(SimulatedCamera::new()))
    }
}

struct SimulatedCamera {
    resolution: usize,
    shutter: usize,
    aperture: usize,
    iso: usize,
    started: Instant,
    last_frame: Instant,
    /// The still part of the live view scene at the current exposure.
    background: Option<(f32, Vec<u8>)>,
}

impl SimulatedCamera {
    fn new() -> Self {
        Self {
            resolution: 0,
            shutter: 6,
            aperture: 3,
            iso: 2,
            started: Instant::now(),
            last_frame: Instant::now(),
            background: None,
        }
    }

    /// Exposure relative to 1/60 at f/5.6 and ISO 400.
    fn gain(&self) -> f32 {
        let time = SHUTTER_SPEEDS[self.shutter].1 * 60.0;
        let iso = ISOS[self.iso].1 / 400.0;
        let aperture = (5.6 / APERTURES[self.aperture].1).powi(2);
        time * iso * aperture
    }

    fn time(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }
}

fn choices(table: &[(&str, f32)]) -> Vec<String> {
    table.iter().map(|(name, _)| (*name).to_owned()).collect()
}

impl Camera for SimulatedCamera {
    fn controls(&self) -> Vec<Control> {
        vec![
            Control {
                setting: Setting::Resolution,
                choices: RESOLUTIONS
                    .iter()
                    .map(|(w, h)| resolution_label(*w, *h))
                    .collect(),
            },
            Control {
                setting: Setting::ShutterSpeed,
                choices: choices(&SHUTTER_SPEEDS),
            },
            Control {
                setting: Setting::Aperture,
                choices: choices(&APERTURES),
            },
            Control {
                setting: Setting::Iso,
                choices: choices(&ISOS),
            },
        ]
    }

    fn settings(&self) -> CameraSettings {
        let (width, height) = RESOLUTIONS[self.resolution];
        CameraSettings {
            resolution: Some(resolution_label(width, height)),
            shutter_speed: Some(SHUTTER_SPEEDS[self.shutter].0.into()),
            aperture: Some(APERTURES[self.aperture].0.into()),
            iso: Some(ISOS[self.iso].0.into()),
            ..CameraSettings::default()
        }
    }

    fn set(&mut self, setting: Setting, value: &str) -> Result<()> {
        let unsupported = || {
            Error::Failed(format!(
                "The simulated camera does not support {} {value}.",
                setting.label()
            ))
        };
        if setting == Setting::Resolution {
            self.resolution = RESOLUTIONS
                .iter()
                .position(|(w, h)| resolution_label(*w, *h) == value)
                .ok_or_else(unsupported)?;
            return Ok(());
        }
        let (table, slot): (&[(&str, f32)], &mut usize) = match setting {
            Setting::ShutterSpeed => (&SHUTTER_SPEEDS, &mut self.shutter),
            Setting::Aperture => (&APERTURES, &mut self.aperture),
            Setting::Iso => (&ISOS, &mut self.iso),
            Setting::Resolution | Setting::Gain | Setting::WhiteBalance | Setting::Focus => {
                (&[], &mut self.iso)
            }
        };
        let index = table
            .iter()
            .position(|(name, _)| *name == value)
            .ok_or_else(unsupported)?;
        *slot = index;
        Ok(())
    }

    fn set_live_view(&mut self, _on: bool) -> Result<()> {
        Ok(())
    }

    fn live_view(&mut self) -> Result<Frame> {
        let wait = LIVE_INTERVAL.saturating_sub(self.last_frame.elapsed());
        thread::sleep(wait);
        self.last_frame = Instant::now();

        let (width, height) = LIVE_SIZE;
        let gain = self.gain();
        if self.background.as_ref().is_none_or(|(g, _)| *g != gain) {
            self.background = Some((gain, render(width, height, gain, None)));
        }
        let mut rgba = self
            .background
            .as_ref()
            .map(|(_, b)| b.clone())
            .unwrap_or_default();
        draw_figure(&mut rgba, width, height, gain, self.time());
        Ok(Frame {
            width,
            height,
            rgba,
        })
    }

    fn capture(&mut self) -> Result<Capture> {
        thread::sleep(CAPTURE_TIME);
        let (width, height) = RESOLUTIONS[self.resolution];
        let rgba = render(width, height, self.gain(), Some(self.time()));
        let rgb = image::RgbaImage::from_raw(width, height, rgba)
            .map(|image| image::DynamicImage::ImageRgba8(image).into_rgb8())
            .ok_or_else(|| Error::Failed("The simulated image has the wrong size.".into()))?;
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), 90)
            .encode_image(&rgb)
            .map_err(|e| Error::Failed(format!("Could not encode the image: {e}")))?;
        Ok(Capture { jpeg, raw: None })
    }
}

/// Renders the scene, with the moving figure at time `figure` if given.
fn render(width: u32, height: u32, gain: f32, figure: Option<f32>) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            rgba.extend_from_slice(&expose(scene(u, v, width as f32 / height as f32), gain));
        }
    }
    if let Some(time) = figure {
        draw_figure(&mut rgba, width, height, gain, time);
    }
    rgba
}

/// The still scene's linear color at `u`, `v` (0 to 1 across and down).
fn scene(u: f32, v: f32, aspect: f32) -> [f32; 3] {
    // Focus target: a star of alternating wedges, fine at the center.
    let (dx, dy) = ((u - 0.5) * aspect, v - 0.38);
    let r = (dx * dx + dy * dy).sqrt();
    if r < 0.13 {
        let wedge = ((dy.atan2(dx) / std::f32::consts::TAU + 0.5) * 48.0) as u32;
        return if wedge.is_multiple_of(2) {
            [0.03; 3]
        } else {
            [0.8; 3]
        };
    }
    if r < 0.135 {
        return [0.03; 3];
    }

    // Blocks standing on the table.
    let blocks = [
        (0.12, 0.30, 0.48, [0.42, 0.08, 0.05]),
        (0.70, 0.86, 0.54, [0.08, 0.16, 0.42]),
        (0.32, 0.40, 0.60, [0.62, 0.48, 0.06]),
    ];
    for (left, right, top, color) in blocks {
        if (left..right).contains(&u) && (top..0.70).contains(&v) {
            let row = ((v - top) / 0.04) as u32;
            let offset = if row.is_multiple_of(2) { 0.0 } else { 0.03 };
            let mortar = (v - top) % 0.04 < 0.004 || (u - left + offset) % 0.06 < 0.004;
            return if mortar { [0.25, 0.24, 0.22] } else { color };
        }
    }

    if v >= 0.70 {
        // A checkered table top.
        let check = ((u * 16.0) as u32 + ((v - 0.70) * 24.0) as u32).is_multiple_of(2);
        return if check {
            [0.36, 0.22, 0.12]
        } else {
            [0.30, 0.18, 0.10]
        };
    }
    // The back wall, lit from above.
    let light = 0.30 - v * 0.15;
    [light, light * 0.97, light * 0.92]
}

/// Draws the moving figure, a green ball that slides along the table.
fn draw_figure(rgba: &mut [u8], width: u32, height: u32, gain: f32, time: f32) {
    let (w, h) = (width as f32, height as f32);
    let cx = (0.5 + 0.32 * (time * 0.8).sin()) * w;
    let cy = 0.64 * h;
    let radius = 0.06 * h;
    let color = expose([0.10, 0.45, 0.12], gain);
    let shade = expose([0.04, 0.18, 0.05], gain);
    let x0 = (cx - radius).max(0.0) as u32;
    let x1 = ((cx + radius) as u32).min(width.saturating_sub(1));
    let y0 = (cy - radius).max(0.0) as u32;
    let y1 = ((cy + radius) as u32).min(height.saturating_sub(1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= radius * radius {
                let i = ((y * width + x) * 4) as usize;
                let lit = dx + dy < 0.0;
                rgba[i..i + 4].copy_from_slice(if lit { &color } else { &shade });
            }
        }
    }
}

/// Applies the exposure gain and the display gamma to a linear color.
fn expose(color: [f32; 3], gain: f32) -> [u8; 4] {
    let channel = |c: f32| ((c * gain).clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
    [channel(color[0]), channel(color[1]), channel(color[2]), 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_change_exposure() {
        let mut camera = SimulatedCamera::new();
        let base = camera.gain();
        assert!((base - 1.0).abs() < 1e-5);
        camera.set(Setting::Iso, "800").unwrap();
        assert!((camera.gain() - 2.0 * base).abs() < 1e-5);
        assert_eq!(camera.settings().iso.as_deref(), Some("800"));
        assert!(camera.set(Setting::Iso, "12").is_err());
    }

    #[test]
    fn trading_keeps_the_brightness() {
        use super::super::exposure::{Triangle, partner, thirds};
        let camera = SimulatedCamera::new();
        let triangle = Triangle::new(&camera.controls());
        assert!(triangle.is_complete());
        let settings = camera.settings();
        let base = camera.gain();
        for locked in [Setting::ShutterSpeed, Setting::Aperture, Setting::Iso] {
            for setting in [Setting::ShutterSpeed, Setting::Aperture, Setting::Iso] {
                let Some(other) = partner(locked, setting) else {
                    continue;
                };
                let current = thirds(setting, settings.get(setting).unwrap()).unwrap();
                for target in [current - 3, current + 3] {
                    let change = triangle
                        .nearest(&settings, setting, target, Some(other))
                        .unwrap();
                    let mut traded = SimulatedCamera::new();
                    for (s, v) in &change {
                        traded.set(*s, v).unwrap();
                    }
                    // Nominal values such as 1/60 are a little off true stops.
                    let error = (traded.gain() / base).log2().abs();
                    assert!(error < 0.2, "{change:?} is {error} stops off");
                }
            }
        }
    }

    #[test]
    fn capture_is_a_jpeg() {
        let mut camera = SimulatedCamera::new();
        let capture = camera.capture().unwrap();
        let image = image::load_from_memory(&capture.jpeg).unwrap();
        assert_eq!((image.width(), image.height()), RESOLUTIONS[0]);
        camera.set(Setting::Resolution, "1200x800").unwrap();
        let capture = camera.capture().unwrap();
        let image = image::load_from_memory(&capture.jpeg).unwrap();
        assert_eq!((image.width(), image.height()), (1200, 800));
    }
}

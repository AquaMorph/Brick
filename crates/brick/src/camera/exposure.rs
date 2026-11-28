//! The exposure triangle: how shutter speed, aperture, and ISO each change
//! the brightness of an image (CT9, CT10).
//!
//! Exposure is counted in thirds of a stop, the smallest step most cameras
//! offer. Camera values are rounded (1/60 is really 1/64, f/5.6 is really
//! f/5.66), so every value is snapped to the nearest third. That makes equal
//! exposures compare exactly equal.

use super::{CameraSettings, Control, Setting};

/// The settings that make up the exposure triangle.
pub const SETTINGS: [Setting; 3] = [Setting::ShutterSpeed, Setting::Aperture, Setting::Iso];

/// How much light a value of an exposure setting lets in, in thirds of a
/// stop. More is brighter. 1 second, f/1, and ISO 100 are zero. `None` for
/// values without a fixed exposure, such as Auto or Bulb, and for settings
/// outside the triangle.
pub fn thirds(setting: Setting, value: &str) -> Option<i32> {
    let stops = match setting {
        Setting::ShutterSpeed => shutter_seconds(value)?.log2(),
        Setting::Aperture => -2.0 * f_number(value)?.log2(),
        Setting::Iso => (iso(value)? / 100.0).log2(),
        Setting::Resolution | Setting::Gain | Setting::WhiteBalance | Setting::Focus => {
            return None;
        }
    };
    let thirds = (stops * 3.0).round();
    // Anything outside this is not a camera setting.
    (thirds.abs() < 1000.0).then_some(thirds as i32)
}

/// Reads shutter speeds such as `1/125`, `1"`, `0.5"`, and Canon's `1"3`
/// for 1.3 seconds.
fn shutter_seconds(value: &str) -> Option<f32> {
    let value = value.trim();
    let seconds = if let Some(denominator) = value.strip_prefix("1/") {
        1.0 / denominator.trim().parse::<f32>().ok()?
    } else if let Some((whole, tenths)) = value.split_once('"') {
        if tenths.is_empty() {
            whole.parse().ok()?
        } else {
            format!("{whole}.{tenths}").parse().ok()?
        }
    } else {
        value
            .strip_suffix('s')
            .unwrap_or(value)
            .trim()
            .parse()
            .ok()?
    };
    (seconds > 0.0 && seconds.is_finite()).then_some(seconds)
}

/// Reads apertures such as `f/5.6`, `F5.6`, and `5.6`.
fn f_number(value: &str) -> Option<f32> {
    let number: f32 = value
        .trim()
        .trim_start_matches(['f', 'F'])
        .trim_start_matches('/')
        .parse()
        .ok()?;
    (number > 0.0 && number.is_finite()).then_some(number)
}

/// Reads ISO speeds such as `400` and `ISO 400`.
fn iso(value: &str) -> Option<f32> {
    let number: f32 = value.trim().trim_start_matches("ISO").trim().parse().ok()?;
    (number > 0.0 && number.is_finite()).then_some(number)
}

/// The values of one exposure setting that have a fixed exposure, darkest
/// first.
#[derive(Debug, Clone, PartialEq)]
pub struct Scale {
    pub setting: Setting,
    steps: Vec<(i32, String)>,
}

impl Scale {
    /// `None` when fewer than two of the control's values have a fixed
    /// exposure, since there is nothing to step between.
    pub fn new(control: &Control) -> Option<Self> {
        let mut steps: Vec<(i32, String)> = control
            .choices
            .iter()
            .filter_map(|choice| Some((thirds(control.setting, choice)?, choice.clone())))
            .collect();
        if steps.len() < 2 {
            return None;
        }
        // Stable, so values that snap to the same third keep the camera's
        // order.
        steps.sort_by_key(|(thirds, _)| *thirds);
        Some(Self {
            setting: control.setting,
            steps,
        })
    }

    /// Every value with its exposure in thirds, darkest first.
    pub fn steps(&self) -> &[(i32, String)] {
        &self.steps
    }

    /// The darkest and brightest exposure on the scale, in thirds.
    pub fn range(&self) -> (i32, i32) {
        let first = self.steps.first().map_or(0, |s| s.0);
        let last = self.steps.last().map_or(0, |s| s.0);
        (first, last)
    }

    /// A value that lets in exactly `thirds` of light.
    fn find(&self, thirds: i32) -> Option<&str> {
        self.steps
            .iter()
            .find(|(t, _)| *t == thirds)
            .map(|(_, v)| v.as_str())
    }
}

/// Settings to change together, such as aperture and the shutter speed that
/// makes up for it (CT10).
pub type Change = Vec<(Setting, String)>;

/// The setting traded against `setting` while `locked` is held still: the
/// third one of the triangle.
pub fn partner(locked: Setting, setting: Setting) -> Option<Setting> {
    if locked == setting || !SETTINGS.contains(&locked) || !SETTINGS.contains(&setting) {
        return None;
    }
    SETTINGS.into_iter().find(|s| *s != locked && *s != setting)
}

/// The exposure settings a camera offers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Triangle {
    scales: Vec<Scale>,
}

impl Triangle {
    pub fn new(controls: &[Control]) -> Self {
        let scales = SETTINGS
            .iter()
            .filter_map(|setting| controls.iter().find(|c| c.setting == *setting))
            .filter_map(Scale::new)
            .collect();
        Self { scales }
    }

    pub fn scale(&self, setting: Setting) -> Option<&Scale> {
        self.scales.iter().find(|s| s.setting == setting)
    }

    /// Shutter speed, aperture, and ISO, as far as the camera has them.
    pub fn scales(&self) -> &[Scale] {
        &self.scales
    }

    /// Whether the camera has all three settings, so one can be locked while
    /// the other two trade (CT10).
    pub fn is_complete(&self) -> bool {
        self.scales.len() == SETTINGS.len()
    }

    /// The total light the settings let in, in thirds of a stop. `None`
    /// when a setting has no fixed exposure, such as Auto ISO.
    pub fn exposure(&self, settings: &CameraSettings) -> Option<i32> {
        if self.scales.is_empty() {
            return None;
        }
        self.scales
            .iter()
            .map(|scale| thirds(scale.setting, settings.get(scale.setting)?))
            .sum()
    }

    /// Every value `setting` can change to, in thirds, with what changes.
    /// With a `partner`, only values the partner can make up for, so the
    /// exposure stays the same.
    fn candidates(
        &self,
        settings: &CameraSettings,
        setting: Setting,
        partner: Option<Setting>,
    ) -> Vec<(i32, Change)> {
        let Some(scale) = self.scale(setting) else {
            return Vec::new();
        };
        let Some(partner) = partner else {
            return scale
                .steps
                .iter()
                .map(|(t, v)| (*t, vec![(setting, v.clone())]))
                .collect();
        };
        let balance = || {
            let other = self.scale(partner)?;
            let current = thirds(setting, settings.get(setting)?)?;
            let other_current = thirds(partner, settings.get(partner)?)?;
            Some(
                scale
                    .steps
                    .iter()
                    .filter_map(|(t, v)| {
                        let matching = other.find(other_current - (t - current))?;
                        Some((
                            *t,
                            vec![(setting, v.clone()), (partner, matching.to_owned())],
                        ))
                    })
                    .collect(),
            )
        };
        balance().unwrap_or_default()
    }

    /// The change that brings `setting` closest to `target` thirds.
    pub fn nearest(
        &self,
        settings: &CameraSettings,
        setting: Setting,
        target: i32,
        partner: Option<Setting>,
    ) -> Option<Change> {
        self.candidates(settings, setting, partner)
            .into_iter()
            .min_by_key(|(t, _)| (t - target).abs())
            .map(|(_, change)| change)
    }

    /// The change that sets `setting` to `value`. `None` when the partner
    /// cannot make up for it. Values without a fixed exposure, such as Auto,
    /// change on their own.
    pub fn change_to(
        &self,
        settings: &CameraSettings,
        setting: Setting,
        value: &str,
        partner: Option<Setting>,
    ) -> Option<Change> {
        let measured = thirds(setting, value).is_some()
            && settings
                .get(setting)
                .and_then(|v| thirds(setting, v))
                .is_some();
        if !measured {
            return Some(vec![(setting, value.to_owned())]);
        }
        self.candidates(settings, setting, partner)
            .into_iter()
            .find(|(_, change)| change[0].1 == value)
            .map(|(_, change)| change)
    }
}

/// Writes a number of thirds of a stop the way photographers say it, such as
/// `1 1/3` or `2/3`, without a sign.
pub fn format_stops(thirds: i32) -> String {
    let whole = thirds.unsigned_abs() / 3;
    let fraction = match thirds.unsigned_abs() % 3 {
        1 => "1/3",
        2 => "2/3",
        _ => "",
    };
    match (whole, fraction) {
        (_, "") => whole.to_string(),
        (0, f) => f.to_owned(),
        (w, f) => format!("{w} {f}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(setting: Setting, choices: &[&str]) -> Control {
        Control {
            setting,
            choices: choices.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    fn canon() -> Vec<Control> {
        vec![
            control(Setting::Resolution, &["6000x4000"]),
            control(
                Setting::ShutterSpeed,
                &[
                    "Bulb", "1\"3", "1\"", "0\"8", "1/2", "1/4", "1/8", "1/15", "1/30", "1/60",
                    "1/80", "1/100", "1/125", "1/250", "1/500",
                ],
            ),
            control(
                Setting::Aperture,
                &[
                    "f/2.8", "f/4", "f/5.6", "f/6.3", "f/7.1", "f/8", "f/11", "f/16",
                ],
            ),
            control(Setting::Iso, &["Auto", "100", "200", "400", "800", "1600"]),
        ]
    }

    fn settings(shutter: &str, aperture: &str, iso: &str) -> CameraSettings {
        CameraSettings {
            shutter_speed: Some(shutter.into()),
            aperture: Some(aperture.into()),
            iso: Some(iso.into()),
            ..CameraSettings::default()
        }
    }

    #[test]
    fn values_snap_to_thirds() {
        assert_eq!(thirds(Setting::ShutterSpeed, "1/60"), Some(-18));
        assert_eq!(thirds(Setting::ShutterSpeed, "1/80"), Some(-19));
        assert_eq!(thirds(Setting::ShutterSpeed, "1\""), Some(0));
        assert_eq!(thirds(Setting::ShutterSpeed, "1\"3"), Some(1));
        assert_eq!(thirds(Setting::ShutterSpeed, "0.5\""), Some(-3));
        assert_eq!(thirds(Setting::ShutterSpeed, "Bulb"), None);
        assert_eq!(thirds(Setting::Aperture, "f/5.6"), Some(-15));
        assert_eq!(thirds(Setting::Aperture, "F1.8"), Some(-5));
        assert_eq!(thirds(Setting::Aperture, "f/11"), Some(-21));
        assert_eq!(thirds(Setting::Iso, "100"), Some(0));
        assert_eq!(thirds(Setting::Iso, "ISO 1600"), Some(12));
        assert_eq!(thirds(Setting::Iso, "Auto"), None);
        assert_eq!(thirds(Setting::Gain, "100"), None);
    }

    #[test]
    fn scales_run_darkest_first() {
        let triangle = Triangle::new(&canon());
        let shutter = triangle.scale(Setting::ShutterSpeed).unwrap();
        assert_eq!(shutter.steps.len(), 14);
        assert_eq!(shutter.steps[0].1, "1/500");
        let aperture = triangle.scale(Setting::Aperture).unwrap();
        assert_eq!(aperture.steps[0].1, "f/16");
    }

    #[test]
    fn exposure_adds_the_three_settings() {
        let triangle = Triangle::new(&canon());
        assert!(triangle.is_complete());
        let base = triangle.exposure(&settings("1/60", "f/5.6", "400"));
        assert_eq!(triangle.exposure(&settings("1/30", "f/8", "400")), base);
        assert_eq!(
            triangle.exposure(&settings("1/60", "f/5.6", "800")),
            base.map(|t| t + 3)
        );
        assert_eq!(triangle.exposure(&settings("1/60", "f/5.6", "Auto")), None);
    }

    #[test]
    fn stops_read_like_a_camera() {
        assert_eq!(format_stops(0), "0");
        assert_eq!(format_stops(2), "2/3");
        assert_eq!(format_stops(-3), "1");
        assert_eq!(format_stops(4), "1 1/3");
    }

    #[test]
    fn partner_is_the_third_setting() {
        assert_eq!(
            partner(Setting::Iso, Setting::Aperture),
            Some(Setting::ShutterSpeed)
        );
        assert_eq!(
            partner(Setting::ShutterSpeed, Setting::Iso),
            Some(Setting::Aperture)
        );
        assert_eq!(partner(Setting::Iso, Setting::Iso), None);
    }

    #[test]
    fn partner_keeps_exposure() {
        let triangle = Triangle::new(&canon());
        let current = settings("1/60", "f/5.6", "400");
        let shutter = Some(Setting::ShutterSpeed);
        let change = |pairs: &[(Setting, &str)]| -> Option<Change> {
            Some(pairs.iter().map(|(s, v)| (*s, (*v).to_owned())).collect())
        };
        // One stop darker on aperture, one stop longer on shutter.
        assert_eq!(
            triangle.nearest(&current, Setting::Aperture, -18, shutter),
            change(&[(Setting::Aperture, "f/8"), (Setting::ShutterSpeed, "1/30")])
        );
        // f/6.3 needs 1/50, which the camera lacks, so the nearest that
        // works is f/5.6 itself.
        assert_eq!(
            triangle.nearest(&current, Setting::Aperture, -16, shutter),
            change(&[
                (Setting::Aperture, "f/5.6"),
                (Setting::ShutterSpeed, "1/60")
            ])
        );
        assert_eq!(
            triangle.change_to(&current, Setting::Aperture, "f/6.3", shutter),
            None
        );
        assert_eq!(
            triangle.change_to(&current, Setting::Iso, "800", shutter),
            change(&[(Setting::Iso, "800"), (Setting::ShutterSpeed, "1/125")])
        );
        // Auto has no exposure to keep.
        assert_eq!(
            triangle.change_to(&current, Setting::Iso, "Auto", shutter),
            change(&[(Setting::Iso, "Auto")])
        );
        // Without a partner the exposure changes.
        assert_eq!(
            triangle.nearest(&current, Setting::Aperture, -16, None),
            change(&[(Setting::Aperture, "f/6.3")])
        );
    }

    #[test]
    fn webcams_cannot_keep_exposure() {
        let triangle = Triangle::new(&[
            control(Setting::ShutterSpeed, &["1/1000", "1/500", "1/250"]),
            control(Setting::Gain, &["0", "10", "20"]),
        ]);
        assert_eq!(triangle.scales().len(), 1);
        assert!(!triangle.is_complete());
    }
}

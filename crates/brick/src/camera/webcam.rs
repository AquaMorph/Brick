//! The values offered for webcam settings, shared by the platform backends
//! that control webcams directly (CT1). Webcams describe each setting as a
//! range of integers, which is turned into a short list of values the user
//! can pick from.

use super::Setting;

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

/// The values a webcam control accepts: every `step` from `minimum` to
/// `maximum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub minimum: i32,
    pub maximum: i32,
    pub step: i32,
}

/// Labels for a setting and the control values they stand for, in
/// increasing order. Exposure time is in units of 100 µs, as both V4L2 and
/// the USB Video Class define it, and white balance in kelvin.
pub fn choices(setting: Setting, range: Range) -> Vec<(String, i32)> {
    match setting {
        Setting::ShutterSpeed => exposure_choices(range),
        Setting::WhiteBalance => kelvin_choices(range),
        _ => steps(range, MAX_STEPS)
            .into_iter()
            .map(|v| (v.to_string(), v))
            .collect(),
    }
}

/// The label of the choice closest to a control value.
pub fn nearest(choices: &[(String, i32)], value: i32) -> Option<&str> {
    choices
        .iter()
        .min_by_key(|(_, v)| (i64::from(*v) - i64::from(value)).abs())
        .map(|(label, _)| label.as_str())
}

fn exposure_choices(range: Range) -> Vec<(String, i32)> {
    let mut choices: Vec<(String, i32)> = SHUTTER_SPEEDS
        .iter()
        .filter_map(|(label, seconds)| {
            let units = snap(range, (seconds * 10_000.0).round() as i32);
            let exact = (f64::from(units) / 10_000.0 - seconds).abs() / seconds < 0.05;
            exact.then(|| ((*label).to_owned(), units))
        })
        .collect();
    choices.dedup_by_key(|(_, v)| *v);
    choices
}

fn kelvin_choices(range: Range) -> Vec<(String, i32)> {
    let mut choices: Vec<(String, i32)> = (range.minimum..=range.maximum)
        .step_by(100)
        .map(|k| snap(range, (k + 99) / 100 * 100))
        .map(|k| (format!("{k} K"), k))
        .collect();
    choices.dedup_by_key(|(_, v)| *v);
    choices
}

/// Up to `count + 1` evenly spaced values across a control's range.
fn steps(range: Range, count: i32) -> Vec<i32> {
    let span = i64::from(range.maximum) - i64::from(range.minimum);
    if span <= 0 {
        return vec![range.minimum];
    }
    let mut values: Vec<i32> = (0..=i64::from(count))
        .map(|i| {
            snap(
                range,
                (i64::from(range.minimum) + span * i / i64::from(count)) as i32,
            )
        })
        .collect();
    values.dedup();
    values
}

/// The closest value to `value` the control accepts.
fn snap(range: Range, value: i32) -> i32 {
    let step = i64::from(range.step.max(1));
    let min = i64::from(range.minimum);
    let offset = (i64::from(value) - min + step / 2).div_euclid(step) * step;
    (min + offset).clamp(min, i64::from(range.maximum)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(minimum: i32, maximum: i32, step: i32) -> Range {
        Range {
            minimum,
            maximum,
            step,
        }
    }

    #[test]
    fn exposure_offers_shutter_speeds_in_range() {
        // 1 ms to 62.5 ms, as on a common laptop webcam.
        let labels: Vec<String> = choices(Setting::ShutterSpeed, range(10, 625, 1))
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
        assert_eq!(steps(range(0, 4, 1), 20), [0, 1, 2, 3, 4]);
        let wide = steps(range(0, 255, 5), 20);
        assert_eq!(wide.first(), Some(&0));
        assert_eq!(wide.last(), Some(&255));
        assert!(wide.iter().all(|v| v % 5 == 0));
        assert_eq!(snap(range(2800, 6500, 10), 4604), 4600);
    }

    #[test]
    fn white_balance_in_hundreds_of_kelvin() {
        let choices = choices(Setting::WhiteBalance, range(2800, 6500, 10));
        assert_eq!(choices.first().map(|c| c.0.as_str()), Some("2800 K"));
        assert_eq!(choices.last().map(|c| c.0.as_str()), Some("6500 K"));
        assert_eq!(choices.len(), 38);
    }

    #[test]
    fn nearest_choice_names_a_value_between_choices() {
        let choices = choices(Setting::Gain, range(0, 255, 1));
        assert_eq!(nearest(&choices, 0), Some("0"));
        assert_eq!(nearest(&choices, 250), Some("255"));
        assert_eq!(nearest(&[], 3), None);
    }
}

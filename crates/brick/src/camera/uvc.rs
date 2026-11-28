//! USB Video Class controls for webcams on macOS (C8, CT1).
//!
//! AVFoundation does not let programs set a webcam's exposure time, gain,
//! white balance, or focus. USB Video Class webcams accept them as requests
//! on their USB control endpoint instead, addressed to the webcam's camera
//! terminal (exposure and focus) and processing unit (gain and white
//! balance). The webcam's descriptors say which controls it has.

use std::io;

use brick_avfoundation::usb::UsbDevice;

use super::webcam::Range;

/// The interface class and subclass of a video control interface.
const VIDEO_CONTROL: (u8, u8) = (0x0e, 0x01);
const INTERFACE: u8 = 0x04;
const CS_INTERFACE: u8 = 0x24;
const INPUT_TERMINAL: u8 = 0x02;
const PROCESSING_UNIT: u8 = 0x05;
const CAMERA_TERMINAL: u16 = 0x0201;
const CONFIGURATION: u16 = 0x02;
/// A class request to an interface.
const CLASS_INTERFACE: u8 = 0x21;
const GET_DESCRIPTOR: u8 = 0x06;
const SET_CUR: u8 = 0x01;
const GET_CUR: u8 = 0x81;
const GET_MIN: u8 = 0x82;
const GET_MAX: u8 = 0x83;
const GET_RES: u8 = 0x84;

/// Auto exposure modes, as bits of the auto exposure mode control.
pub const EXPOSURE_MANUAL: i32 = 0x01;
pub const EXPOSURE_AUTO: i32 = 0x02;
pub const EXPOSURE_APERTURE_PRIORITY: i32 = 0x08;

/// The part of the webcam a control belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    CameraTerminal,
    ProcessingUnit,
}

/// A control this program uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// One of the `EXPOSURE_*` modes.
    AutoExposureMode,
    /// Whether the webcam may lower the frame rate to expose longer.
    AutoExposurePriority,
    /// In units of 100 µs.
    ExposureTime,
    FocusAbsolute,
    FocusAuto,
    Gain,
    /// In kelvin.
    WhiteBalanceTemperature,
    WhiteBalanceAuto,
}

impl Control {
    /// The unit, control selector, value size in bytes, and bit in the
    /// unit's `bmControls`, from the USB Video Class 1.1 specification.
    fn spec(self) -> (Unit, u8, u16, u32) {
        use Unit::{CameraTerminal as Ct, ProcessingUnit as Pu};
        match self {
            Control::AutoExposureMode => (Ct, 0x02, 1, 1),
            Control::AutoExposurePriority => (Ct, 0x03, 1, 2),
            Control::ExposureTime => (Ct, 0x04, 4, 3),
            Control::FocusAbsolute => (Ct, 0x06, 2, 5),
            Control::FocusAuto => (Ct, 0x08, 1, 17),
            Control::Gain => (Pu, 0x04, 2, 9),
            Control::WhiteBalanceTemperature => (Pu, 0x0a, 2, 6),
            Control::WhiteBalanceAuto => (Pu, 0x0b, 1, 12),
        }
    }
}

/// Where a webcam's controls are found, read from its configuration
/// descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    interface: u8,
    /// The camera terminal's ID and the controls it has.
    terminal: Option<(u8, u32)>,
    /// The processing unit's ID and the controls it has.
    processing: Option<(u8, u32)>,
}

/// A USB Video Class webcam's controls.
pub struct Uvc {
    usb: UsbDevice,
    layout: Layout,
}

impl Uvc {
    /// The controls of the webcam with an AVFoundation unique ID, if it is a
    /// USB Video Class webcam.
    pub fn open(unique_id: &str) -> Option<Uvc> {
        let (location, vendor, product) = usb_address(unique_id)?;
        let usb = UsbDevice::find(location, vendor, product).ok()?;
        let header = usb
            .control_in(0x00, GET_DESCRIPTOR, CONFIGURATION << 8, 0, 9)
            .ok()?;
        let total = u16::from_le_bytes([*header.get(2)?, *header.get(3)?]);
        let descriptor = usb
            .control_in(0x00, GET_DESCRIPTOR, CONFIGURATION << 8, 0, total)
            .ok()?;
        let layout = layout(&descriptor)?;
        Some(Uvc { usb, layout })
    }

    /// Whether the webcam says it has the control.
    pub fn has(&self, control: Control) -> bool {
        self.address(control).is_some()
    }

    pub fn get(&self, control: Control) -> io::Result<i32> {
        self.read(control, GET_CUR)
    }

    pub fn set(&self, control: Control, value: i32) -> io::Result<()> {
        let (_, selector, length, _) = control.spec();
        let index = self.address(control).ok_or_else(unsupported)?;
        let bytes = value.to_le_bytes();
        self.usb.control_out(
            CLASS_INTERFACE,
            SET_CUR,
            u16::from(selector) << 8,
            index,
            &bytes[..usize::from(length)],
        )
    }

    pub fn range(&self, control: Control) -> io::Result<Range> {
        Ok(Range {
            minimum: self.read(control, GET_MIN)?,
            maximum: self.read(control, GET_MAX)?,
            step: self.read(control, GET_RES)?,
        })
    }

    /// The `EXPOSURE_*` modes the webcam offers, as bits.
    pub fn exposure_modes(&self) -> i32 {
        self.read(Control::AutoExposureMode, GET_RES).unwrap_or(0)
    }

    fn read(&self, control: Control, request: u8) -> io::Result<i32> {
        let (_, selector, length, _) = control.spec();
        let index = self.address(control).ok_or_else(unsupported)?;
        let data = self.usb.control_in(
            CLASS_INTERFACE,
            request,
            u16::from(selector) << 8,
            index,
            length,
        )?;
        if data.len() != usize::from(length) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the webcam sent a short answer",
            ));
        }
        Ok(unsigned(&data))
    }

    /// The `wIndex` of requests for a control: the unit ID and interface.
    fn address(&self, control: Control) -> Option<u16> {
        let (unit, _, _, bit) = control.spec();
        let (id, controls) = match unit {
            Unit::CameraTerminal => self.layout.terminal,
            Unit::ProcessingUnit => self.layout.processing,
        }?;
        (controls & (1 << bit) != 0).then(|| u16::from(id) << 8 | u16::from(self.layout.interface))
    }
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "the webcam does not have this control",
    )
}

/// A little endian unsigned value of up to four bytes. Values that do not
/// fit an `i32` are capped.
fn unsigned(data: &[u8]) -> i32 {
    let mut bytes = [0u8; 4];
    let n = data.len().min(4);
    bytes[..n].copy_from_slice(&data[..n]);
    i32::try_from(u32::from_le_bytes(bytes)).unwrap_or(i32::MAX)
}

/// The USB location ID, vendor ID, and product ID in an AVFoundation unique
/// ID. macOS names USB webcams `0x` followed by those three in hex, as in
/// `0x10340000046d0824`.
fn usb_address(unique_id: &str) -> Option<(u32, u16, u16)> {
    let hex = unique_id.strip_prefix("0x")?;
    if hex.len() != 16 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        u32::from_str_radix(&hex[..8], 16).ok()?,
        u16::from_str_radix(&hex[8..12], 16).ok()?,
        u16::from_str_radix(&hex[12..], 16).ok()?,
    ))
}

/// Finds the video control interface and its camera terminal and processing
/// unit in a configuration descriptor.
fn layout(descriptor: &[u8]) -> Option<Layout> {
    let mut found: Option<Layout> = None;
    let mut in_video_control = false;
    let mut rest = descriptor;
    while let [length, kind, ..] = *rest {
        let length = usize::from(length);
        if length < 2 || length > rest.len() {
            break;
        }
        let d = &rest[..length];
        rest = &rest[length..];
        match kind {
            INTERFACE if d.len() >= 7 => {
                in_video_control = (d[5], d[6]) == VIDEO_CONTROL && found.is_none();
                if in_video_control {
                    found = Some(Layout {
                        interface: d[2],
                        terminal: None,
                        processing: None,
                    });
                }
            }
            CS_INTERFACE if in_video_control && d.len() >= 3 => {
                let Some(layout) = found.as_mut() else {
                    continue;
                };
                match d[2] {
                    INPUT_TERMINAL if d.len() >= 15 => {
                        if u16::from_le_bytes([d[4], d[5]]) == CAMERA_TERMINAL {
                            layout.terminal = Some((d[3], bitmap(d, 14)));
                        }
                    }
                    PROCESSING_UNIT if d.len() >= 8 => {
                        layout.processing = Some((d[3], bitmap(d, 7)));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    found
}

/// A `bmControls` bitmap whose size is the byte at `size_at`.
fn bitmap(d: &[u8], size_at: usize) -> u32 {
    let size = usize::from(d[size_at]);
    let start = size_at + 1;
    let end = (start + size).min(d.len());
    unsigned(&d[start.min(end)..end]) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The video control part of a real webcam's configuration descriptor.
    #[rustfmt::skip]
    const DESCRIPTOR: &[u8] = &[
        // Configuration.
        0x09, 0x02, 0x00, 0x00, 0x02, 0x01, 0x00, 0x80, 0xfa,
        // Interface association.
        0x08, 0x0b, 0x00, 0x02, 0x0e, 0x03, 0x00, 0x02,
        // Video control interface 0.
        0x09, 0x04, 0x00, 0x00, 0x01, 0x0e, 0x01, 0x00, 0x02,
        // Header.
        0x0d, 0x24, 0x01, 0x00, 0x01, 0xd9, 0x00, 0x00, 0x6c, 0xdc, 0x02, 0x01, 0x01,
        // Camera terminal 1: auto exposure mode and priority, exposure time.
        0x12, 0x24, 0x02, 0x01, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x03, 0x0e, 0x00, 0x00,
        // Processing unit 2: gain, white balance, and more.
        0x0b, 0x24, 0x05, 0x02, 0x01, 0x00, 0x40, 0x02, 0x5b, 0x17, 0x00,
        // Output terminal.
        0x09, 0x24, 0x03, 0x05, 0x01, 0x01, 0x00, 0x04, 0x00,
        // Video streaming interface 1.
        0x09, 0x04, 0x01, 0x00, 0x00, 0x0e, 0x02, 0x00, 0x00,
        0x0e, 0x24, 0x01, 0x01, 0x00, 0x00, 0x81, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00,
    ];

    #[test]
    fn layout_finds_the_terminal_and_processing_unit() {
        let layout = layout(DESCRIPTOR).unwrap();
        assert_eq!(layout.interface, 0);
        assert_eq!(layout.terminal, Some((1, 0x0e)));
        assert_eq!(layout.processing, Some((2, 0x175b)));
    }

    #[test]
    fn controls_follow_the_descriptor_bits() {
        let layout = layout(DESCRIPTOR).unwrap();
        let has = |control: Control| {
            let (unit, _, _, bit) = control.spec();
            let (_, bits) = match unit {
                Unit::CameraTerminal => layout.terminal,
                Unit::ProcessingUnit => layout.processing,
            }
            .unwrap();
            bits & (1 << bit) != 0
        };
        assert!(has(Control::AutoExposureMode));
        assert!(has(Control::ExposureTime));
        assert!(has(Control::Gain));
        assert!(has(Control::WhiteBalanceTemperature));
        assert!(has(Control::WhiteBalanceAuto));
        assert!(!has(Control::FocusAbsolute));
        assert!(!has(Control::FocusAuto));
    }

    #[test]
    fn layout_needs_a_video_control_interface() {
        assert_eq!(layout(&DESCRIPTOR[..9]), None);
        // A truncated descriptor stops parsing instead of reading past it.
        assert!(layout(&DESCRIPTOR[..40]).is_some());
    }

    #[test]
    fn unique_ids_hold_the_usb_address() {
        assert_eq!(
            usb_address("0x10340000046d0824"),
            Some((0x1034_0000, 0x046d, 0x0824))
        );
        assert_eq!(
            usb_address("0x8020000005ac8514"),
            Some((0x8020_0000, 0x05ac, 0x8514))
        );
        assert_eq!(usb_address("47B4B64B-7067-4B9C-AD2B-AE273A71F4B5"), None);
        assert_eq!(usb_address("0x1034"), None);
    }

    #[test]
    fn values_are_little_endian_unsigned() {
        assert_eq!(unsigned(&[0x10, 0x27]), 10_000);
        assert_eq!(unsigned(&[0xff]), 255);
        assert_eq!(unsigned(&[0x2c, 0x01, 0x00, 0x00]), 300);
        assert_eq!(unsigned(&[0xff, 0xff, 0xff, 0xff]), i32::MAX);
    }
}

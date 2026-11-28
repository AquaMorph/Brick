//! The Help menu (F11-F16): the keyboard shortcut list, bug reports, the log
//! folder, and About Brick.

use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

use eframe::egui::{self, RichText};

use crate::{animation, file_menu, theme};

/// Where the project lives, linked from About Brick (F16).
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
/// The version shown in About Brick and in bug reports (F14, F16).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Brick's own licence (R5).
const LICENCE: &str = include_str!("../../../LICENSE");
/// The third party components built into the program, one
/// `name<TAB>version<TAB>licence` line each. Written by
/// `cargo xtask licences` (B1).
const THIRD_PARTY: &str = include_str!("../third_party_licences.tsv");

/// Shortcuts with what they do, as listed in Keyboard Shortcuts (F13).
pub type ShortcutList = &'static [(egui::KeyboardShortcut, &'static str)];

/// Something picked from the Help menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpAction {
    Shortcuts,
    ReportBug,
    OpenLogFolder,
    About,
}

impl HelpAction {
    /// The Help menu entries in order (F12).
    pub const MENU: &[(HelpAction, &str)] = &[
        (HelpAction::Shortcuts, "Keyboard Shortcuts"),
        (HelpAction::ReportBug, "Report a Bug…"),
        (HelpAction::OpenLogFolder, "Open Log Folder"),
        (HelpAction::About, "About Brick"),
    ];
}

/// A dialog opened from the Help menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpDialog {
    Shortcuts,
    About,
}

impl HelpDialog {
    /// Shows the dialog. Returns whether it is still open.
    pub fn show(self, ctx: &egui::Context) -> Result<bool, String> {
        let mut close = false;
        let mut error = None;
        let modal = egui::Modal::new(egui::Id::unique("help_dialog")).show(ctx, |ui| match self {
            HelpDialog::Shortcuts => {
                ui.set_width(440.0);
                shortcuts(ui);
                ui.add_space(12.0);
                close = close_button(ui);
            }
            HelpDialog::About => {
                ui.set_width(520.0);
                if let Err(e) = about(ui) {
                    error = Some(e);
                }
                ui.add_space(12.0);
                close = close_button(ui);
            }
        });
        let open = !(close || modal.should_close());
        match error {
            Some(e) => Err(e),
            None => Ok(open),
        }
    }
}

fn close_button(ui: &mut egui::Ui) -> bool {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        ui.button("Close").clicked()
    })
    .inner
}

/// Every shortcut grouped by where it works (F13).
fn shortcuts(ui: &mut egui::Ui) {
    ui.label(RichText::new("Keyboard Shortcuts").size(17.0).strong());
    let groups: [(&str, &str, ShortcutList); 2] = [
        ("File", "Every tab", file_menu::SHORTCUTS),
        ("Animation", "Animation tab", animation::SHORTCUTS),
    ];
    for (title, scope, list) in groups {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(scope).color(theme::TEXT_DIM));
        });
        ui.add_space(4.0);
        egui::Grid::new(("shortcuts", title))
            .num_columns(2)
            .spacing([24.0, 6.0])
            .show(ui, |ui| {
                for (shortcut, label) in list {
                    let keys = ui.ctx().format_shortcut(shortcut);
                    ui.label(RichText::new(keys).monospace().color(theme::TEXT_DIM));
                    ui.label(*label);
                    ui.end_row();
                }
            });
    }
}

/// The version, licences, and repository link (F16).
fn about(ui: &mut egui::Ui) -> Result<(), String> {
    let mut result = Ok(());
    ui.label(RichText::new("Brick").size(22.0).strong());
    ui.label(RichText::new(format!("Version {VERSION}")).color(theme::TEXT_DIM));
    ui.add_space(4.0);
    ui.label(env!("CARGO_PKG_DESCRIPTION"));
    if ui.link(REPOSITORY).clicked() {
        result = open_external(REPOSITORY)
            .map_err(|e| format!("Could not open {REPOSITORY} in the browser: {e}"));
    }

    ui.add_space(12.0);
    egui::CollapsingHeader::new("Licence")
        .default_open(true)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("licence")
                .max_height(160.0)
                .show(ui, |ui| ui.label(RichText::new(LICENCE).small()));
        });
    egui::CollapsingHeader::new(format!(
        "Third party components ({})",
        THIRD_PARTY.lines().count()
    ))
    .show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("third_party")
            .max_height(220.0)
            .show(ui, |ui| {
                egui::Grid::new("third_party_grid")
                    .num_columns(2)
                    .spacing([24.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for line in THIRD_PARTY.lines() {
                            let mut parts = line.split('\t');
                            let (name, version, licence) =
                                (parts.next(), parts.next(), parts.next());
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    name.unwrap_or_default(),
                                    version.unwrap_or_default()
                                ))
                                .small(),
                            );
                            ui.label(
                                RichText::new(licence.unwrap_or_default())
                                    .small()
                                    .color(theme::TEXT_DIM),
                            );
                            ui.end_row();
                        }
                    });
            });
    });
    result
}

/// Opens the new issue page with the version and operating system filled in
/// (F14). The browser does the rest; Brick itself sends nothing (R12).
pub fn report_bug() -> io::Result<()> {
    open_external(&bug_report_url(&os_description()))
}

fn bug_report_url(os: &str) -> String {
    let body = format!(
        "**What happened**\n\n\n\
         **Steps to reproduce**\n\n1. \n\n\
         **What you expected**\n\n\n\
         **System**\n\n\
         - Brick version: {VERSION}\n\
         - Operating system: {os}\n"
    );
    format!("{REPOSITORY}/issues/new?body={}", percent_encode(&body))
}

/// Encodes everything but the URL unreserved characters.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The operating system's name and version, for bug reports (F14).
fn os_description() -> String {
    os_version().unwrap_or_else(|| std::env::consts::OS.to_owned())
}

#[cfg(target_os = "linux")]
fn os_version() -> Option<String> {
    let release = std::fs::read_to_string("/etc/os-release").ok();
    let name = release
        .as_deref()
        .and_then(|r| r.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")))
        .map(|n| n.trim_matches('"').to_owned())
        .unwrap_or_else(|| "Linux".to_owned());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").ok();
    Some(match kernel {
        Some(k) => format!("{name} (Linux {})", k.trim()),
        None => name,
    })
}

#[cfg(target_os = "macos")]
fn os_version() -> Option<String> {
    let output = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    let version = String::from_utf8(output.stdout).ok()?;
    Some(format!("macOS {}", version.trim()))
}

#[cfg(target_os = "windows")]
fn os_version() -> Option<String> {
    use std::os::windows::process::CommandExt;
    /// Keeps a console window from flashing up.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("cmd")
        .args(["/c", "ver"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!version.is_empty()).then_some(version)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn os_version() -> Option<String> {
    None
}

/// The operating system's standard log directory for Brick (B4).
pub fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        directories::BaseDirs::new().map(|dirs| dirs.home_dir().join("Library/Logs/Brick"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        directories::ProjectDirs::from("", "", "Brick").map(|dirs| {
            dirs.state_dir()
                .unwrap_or_else(|| dirs.data_local_dir())
                .join("logs")
        })
    }
}

/// Shows the log directory in the file manager, creating it if no log has
/// been written yet (F15).
pub fn open_log_folder() -> Result<(), String> {
    let dir = log_dir().ok_or("This system has no log directory.")?;
    std::fs::create_dir_all(&dir)
        .and_then(|()| open_external(&dir.to_string_lossy()))
        .map_err(|e| format!("Could not open the log folder {}: {e}", dir.display()))
}

/// Opens a URL in the default browser or a folder in the file manager.
fn open_external(target: &str) -> io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(target_os = "windows") {
        Command::new("explorer")
    } else {
        Command::new("xdg-open")
    };
    let mut child = command
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Reap the opener when it exits so it does not linger.
    thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bug_report_fills_in_the_system() {
        let url = bug_report_url("Plan 9 & friends");
        assert!(url.starts_with("https://github.com/AquaMorph/Brick/issues/new?body="));
        let body = url.split_once("?body=").unwrap().1;
        assert!(!body.contains(['&', ' ', '\n', '#']));
        assert!(body.contains(&percent_encode(&format!("Brick version: {VERSION}"))));
        assert!(body.contains("Plan%209%20%26%20friends"));
    }

    #[test]
    fn percent_encoding_keeps_unreserved_characters() {
        assert_eq!(percent_encode("a-Z_0.~"), "a-Z_0.~");
        assert_eq!(percent_encode("a b/é"), "a%20b%2F%C3%A9");
    }

    #[test]
    fn about_lists_bundled_components() {
        assert!(THIRD_PARTY.lines().count() > 0);
        for line in THIRD_PARTY.lines() {
            assert_eq!(line.split('\t').count(), 3, "{line}");
        }
        assert!(!THIRD_PARTY.lines().any(|l| l.starts_with("brick\t")));
    }

    #[test]
    fn every_help_action_is_in_the_menu() {
        for action in [
            HelpAction::Shortcuts,
            HelpAction::ReportBug,
            HelpAction::OpenLogFolder,
            HelpAction::About,
        ] {
            assert!(HelpAction::MENU.iter().any(|(a, _)| *a == action));
        }
    }
}

//! The File and Help menus (F1, F2, F11, F12) and the File menu shortcuts
//! (F10).
//!
//! On macOS the menus live in the system menu bar, which also handles the
//! shortcuts. On Linux and Windows they are drawn in a menu bar above the tab
//! bar and the shortcuts are read from egui input.

use std::path::PathBuf;

#[cfg(not(target_os = "macos"))]
use eframe::egui;
use eframe::egui::{Key, KeyboardShortcut, Modifiers};

use crate::help::HelpAction;
use crate::settings::RecentProject;

/// Something the user picked from the menu bar, the project picker, or a
/// shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAction {
    New,
    Open,
    OpenRecent(PathBuf),
    ClearRecent,
    SaveAs,
    Close,
    Quit,
    Help(HelpAction),
}

const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
const CLOSE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
const QUIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);

/// The File menu shortcuts as listed in Help > Keyboard Shortcuts (F13).
pub const SHORTCUTS: &[(KeyboardShortcut, &str)] = &[
    (NEW, "New project"),
    (OPEN, "Open project"),
    (SAVE_AS, "Save project as"),
    (CLOSE, "Close project"),
    (QUIT, "Quit"),
];

/// Reads the File menu shortcuts. They work on every tab, even while a text
/// field has focus (F10).
#[cfg(not(target_os = "macos"))]
pub fn shortcuts(ctx: &egui::Context, has_project: bool) -> Option<FileAction> {
    ctx.input_mut(|input| {
        if input.consume_shortcut(&SAVE_AS) {
            has_project.then_some(FileAction::SaveAs)
        } else if input.consume_shortcut(&NEW) {
            Some(FileAction::New)
        } else if input.consume_shortcut(&OPEN) {
            Some(FileAction::Open)
        } else if input.consume_shortcut(&CLOSE) {
            has_project.then_some(FileAction::Close)
        } else if input.consume_shortcut(&QUIT) {
            Some(FileAction::Quit)
        } else {
            None
        }
    })
}

/// The menu bar drawn above the tab bar on Linux and Windows (F1, F11).
#[cfg(not(target_os = "macos"))]
pub fn menu_bar(
    ui: &mut egui::Ui,
    has_project: bool,
    recent: &[RecentProject],
) -> Option<FileAction> {
    let mut action = None;
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            ui.set_min_width(240.0);
            if menu_item(ui, "New Project…", &NEW, true) {
                action = Some(FileAction::New);
            }
            if menu_item(ui, "Open Project…", &OPEN, true) {
                action = Some(FileAction::Open);
            }
            ui.menu_button("Open Recent", |ui| {
                if let Some(picked) = recent_items(ui, recent) {
                    action = Some(picked);
                }
            });
            ui.separator();
            if menu_item(ui, "Save Project As…", &SAVE_AS, has_project) {
                action = Some(FileAction::SaveAs);
            }
            ui.separator();
            if menu_item(ui, "Close Project", &CLOSE, has_project) {
                action = Some(FileAction::Close);
            }
            ui.separator();
            if menu_item(ui, "Quit", &QUIT, true) {
                action = Some(FileAction::Quit);
            }
        });
        ui.menu_button("Help", |ui| {
            ui.set_min_width(240.0);
            for (help, label) in HelpAction::MENU {
                if ui.button(*label).clicked() {
                    action = Some(FileAction::Help(*help));
                }
            }
        });
    });
    action
}

/// A menu entry showing its shortcut. Returns whether it was clicked.
#[cfg(not(target_os = "macos"))]
fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: &KeyboardShortcut, enabled: bool) -> bool {
    let shortcut = ui.ctx().format_shortcut(shortcut);
    let button = egui::Button::new(label).shortcut_text(shortcut);
    ui.add_enabled(enabled, button).clicked()
}

/// The Open Recent submenu (F5).
#[cfg(not(target_os = "macos"))]
fn recent_items(ui: &mut egui::Ui, recent: &[RecentProject]) -> Option<FileAction> {
    ui.set_min_width(320.0);
    let mut action = None;
    if recent.is_empty() {
        ui.add_enabled(false, egui::Button::new("No Recent Projects"));
    }
    for project in recent {
        let button = egui::Button::new(recent_label(project));
        if ui.add_enabled(project.exists(), button).clicked() {
            action = Some(FileAction::OpenRecent(project.path.clone()));
        }
    }
    ui.separator();
    if ui
        .add_enabled(!recent.is_empty(), egui::Button::new("Clear Menu"))
        .clicked()
    {
        action = Some(FileAction::ClearRecent);
    }
    action
}

/// A recent project's name and path (F5).
pub fn recent_label(project: &RecentProject) -> String {
    format!("{}  —  {}", project.name, project.path.display())
}

#[cfg(target_os = "macos")]
pub use native::NativeMenu;

/// The File and Help menus in the macOS system menu bar (F1, F11).
#[cfg(target_os = "macos")]
mod native {
    use std::sync::{Arc, mpsc};

    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::{FileAction, HelpAction, RecentProject, recent_label};

    const CLEAR_RECENT: &str = "clear_recent";
    const RECENT_PREFIX: &str = "recent:";
    const HELP_PREFIX: &str = "help:";

    pub struct NativeMenu {
        _menu: Menu,
        recent: Submenu,
        save_as: MenuItem,
        close: MenuItem,
        /// What the menu currently shows, to rebuild it only on change.
        shown: Option<(bool, Vec<(RecentProject, bool)>)>,
        events: mpsc::Receiver<MenuEvent>,
    }

    impl NativeMenu {
        /// Installs the menu bar. Must run on the main thread after the
        /// application has launched.
        pub fn install(ctx: &eframe::egui::Context) -> muda::Result<Self> {
            let cmd = |code| Some(Accelerator::new(Modifiers::META, code));
            let new = MenuItem::with_id("new", "New Project…", true, cmd(Code::KeyN));
            let open = MenuItem::with_id("open", "Open Project…", true, cmd(Code::KeyO));
            let recent = Submenu::new("Open Recent", true);
            let save_as = MenuItem::with_id(
                "save_as",
                "Save Project As…",
                false,
                Some(Accelerator::new(
                    Modifiers::META | Modifiers::SHIFT,
                    Code::KeyS,
                )),
            );
            let close = MenuItem::with_id("close", "Close Project", false, cmd(Code::KeyW));
            let quit = MenuItem::with_id("quit", "Quit Brick", true, cmd(Code::KeyQ));

            let help_item = |index: usize, label: &str| {
                MenuItem::with_id(format!("{HELP_PREFIX}{index}"), label, true, None)
            };
            // About Brick goes in the application menu on macOS (F12).
            let about = HelpAction::MENU
                .iter()
                .position(|(action, _)| *action == HelpAction::About)
                .expect("the Help menu has About Brick");
            let app = Submenu::new("Brick", true);
            app.append_items(&[
                &help_item(about, HelpAction::MENU[about].1),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
            ])?;
            let file = Submenu::new("File", true);
            file.append_items(&[
                &new,
                &open,
                &recent,
                &PredefinedMenuItem::separator(),
                &save_as,
                &PredefinedMenuItem::separator(),
                &close,
                &PredefinedMenuItem::separator(),
                &quit,
            ])?;
            let help = Submenu::new("Help", true);
            for (index, (action, label)) in HelpAction::MENU.iter().enumerate() {
                if *action != HelpAction::About {
                    help.append(&help_item(index, label))?;
                }
            }
            let menu = Menu::new();
            menu.append_items(&[&app, &file, &help])?;
            menu.init_for_nsapp();
            help.set_as_help_menu_for_nsapp();

            // Menu events arrive outside the egui event loop, so wake it up.
            let (sender, events) = mpsc::channel();
            let ctx = ctx.clone();
            MenuEvent::set_event_handler(Some(Arc::new(move |event| {
                let _ = sender.send(event);
                ctx.request_repaint();
            })));

            Ok(Self {
                _menu: menu,
                recent,
                save_as,
                close,
                shown: None,
                events,
            })
        }

        /// Brings the enabled items and the Open Recent submenu up to date.
        pub fn update(&mut self, has_project: bool, recent: &[RecentProject]) {
            let wanted: (bool, Vec<(RecentProject, bool)>) = (
                has_project,
                recent.iter().map(|r| (r.clone(), r.exists())).collect(),
            );
            if self.shown.as_ref() == Some(&wanted) {
                return;
            }
            self.save_as.set_enabled(has_project);
            self.close.set_enabled(has_project);

            while self.recent.remove_at(0).is_some() {}
            if wanted.1.is_empty() {
                let none = MenuItem::new("No Recent Projects", false, None);
                let _ = self.recent.append(&none);
            }
            for (index, (project, exists)) in wanted.1.iter().enumerate() {
                let item = MenuItem::with_id(
                    format!("{RECENT_PREFIX}{index}"),
                    recent_label(project),
                    *exists,
                    None,
                );
                let _ = self.recent.append(&item);
            }
            let _ = self.recent.append(&PredefinedMenuItem::separator());
            let clear = MenuItem::with_id(CLEAR_RECENT, "Clear Menu", !wanted.1.is_empty(), None);
            let _ = self.recent.append(&clear);
            self.shown = Some(wanted);
        }

        /// The next action picked from the menu, if any.
        pub fn poll(&mut self) -> Option<FileAction> {
            while let Ok(event) = self.events.try_recv() {
                let id = event.id.as_ref();
                let action = match id {
                    "new" => Some(FileAction::New),
                    "open" => Some(FileAction::Open),
                    "save_as" => Some(FileAction::SaveAs),
                    "close" => Some(FileAction::Close),
                    "quit" => Some(FileAction::Quit),
                    CLEAR_RECENT => Some(FileAction::ClearRecent),
                    _ if id.starts_with(HELP_PREFIX) => id
                        .strip_prefix(HELP_PREFIX)
                        .and_then(|i| i.parse::<usize>().ok())
                        .and_then(|i| HelpAction::MENU.get(i))
                        .map(|(action, _)| FileAction::Help(*action)),
                    _ => id
                        .strip_prefix(RECENT_PREFIX)
                        .and_then(|i| i.parse::<usize>().ok())
                        .and_then(|i| self.shown.as_ref()?.1.get(i))
                        .map(|(project, _)| FileAction::OpenRecent(project.path.clone())),
                };
                if action.is_some() {
                    return action;
                }
            }
            None
        }
    }
}

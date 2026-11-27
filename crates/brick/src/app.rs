//! The main window: File menu, tab bar, active tab, and status bar (U3, W2),
//! and opening, creating, copying, and closing projects (F1-F10).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eframe::egui::{self, Align, Layout, RichText};

use crate::camera::{CameraEvent, Cameras, Connection, Setting};
use crate::cinematography::{self, CinematographyTab};
use crate::direct::DirectTab;
use crate::file_menu::{self, FileAction};
use crate::picker;
use crate::project::{self, Project, ShotKey};
use crate::project_dialog::{DialogKind, DialogResult, ProjectDialog};
use crate::settings::Settings;
use crate::theme;

/// The stages of a project, in the order the user moves through them (W2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Direct,
    Cinematography,
    Animation,
    Export,
}

impl Tab {
    const ALL: [Tab; 4] = [
        Tab::Direct,
        Tab::Cinematography,
        Tab::Animation,
        Tab::Export,
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Direct => "Direct",
            Tab::Cinematography => "Cinematography",
            Tab::Animation => "Animation",
            Tab::Export => "Export",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Tab::Direct => "Break the film into scenes, shots, and takes.",
            Tab::Cinematography => "Adjust camera settings and take test shots.",
            Tab::Animation => "Capture and play back frames for the active take.",
            Tab::Export => "Write frames out of the project.",
        }
    }
}

pub struct BrickApp {
    tab: Tab,
    project: Option<Project>,
    direct: DirectTab,
    cinematography: CinematographyTab,
    cameras: Cameras,
    /// The shot a running test shot capture belongs to (CT5).
    pending_test: Option<ShotKey>,
    /// Setting changes the user asked for that the camera has not confirmed
    /// yet, with the shot to save them to (CT7).
    pending_settings: Vec<(ShotKey, Setting, String)>,
    /// The shot whose camera settings were last applied to the open camera
    /// (CT7).
    applied_shot: Option<ShotKey>,
    settings: Settings,
    dialog: Option<ProjectDialog>,
    copy: Option<CopyJob>,
    /// The window title last sent to the window.
    title: String,
    #[cfg(target_os = "macos")]
    native_menu: Option<file_menu::NativeMenu>,
    /// Errors shown at the top of the window until dismissed (U5).
    errors: Vec<String>,
}

/// How long closing a project waits for a running capture to be written (F9).
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);

/// A Save Project As copy running on a background thread (F6).
struct CopyJob {
    dest: PathBuf,
    done: mpsc::Receiver<project::Result<()>>,
}

impl BrickApp {
    /// `from_args` is the project given on the command line. Without one the
    /// last opened project is opened (R9).
    pub fn new(ctx: &egui::Context, from_args: Option<project::Result<Project>>) -> Self {
        let (settings, settings_error) = Settings::load();
        let cameras = Cameras::new(ctx, settings.last_camera().map(str::to_owned));
        let mut app = Self {
            tab: Tab::Direct,
            project: None,
            direct: DirectTab::default(),
            cinematography: CinematographyTab::default(),
            cameras,
            pending_test: None,
            pending_settings: Vec::new(),
            applied_shot: None,
            settings,
            dialog: None,
            copy: None,
            title: String::new(),
            #[cfg(target_os = "macos")]
            native_menu: None,
            errors: Vec::new(),
        };
        if let Some(e) = settings_error {
            app.errors
                .push(format!("Could not read the application settings: {e}"));
        }
        #[cfg(target_os = "macos")]
        match file_menu::NativeMenu::install(ctx) {
            Ok(menu) => app.native_menu = Some(menu),
            Err(e) => app
                .errors
                .push(format!("Could not create the menu bar: {e}")),
        }
        #[cfg(not(target_os = "macos"))]
        let _ = ctx;

        match from_args {
            Some(Ok(project)) => app.set_project(project),
            Some(Err(e)) => app.errors.push(format!("Could not open the project: {e}")),
            None => app.open_last_project(),
        }
        app
    }

    /// Opens the last opened project, or explains why it could not be opened
    /// and leaves the project picker showing (R9).
    fn open_last_project(&mut self) {
        let Some(last) = self.settings.recent().first().cloned() else {
            return;
        };
        if !last.exists() {
            self.errors.push(format!(
                "The last project, {}, no longer exists.",
                last.path.display()
            ));
            return;
        }
        match Project::open(&last.path) {
            Ok(project) => self.set_project(project),
            Err(e) => self.errors.push(format!(
                "Could not open the last project, {}: {e}",
                last.path.display()
            )),
        }
    }
}

impl eframe::App for BrickApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.update_title(&ctx);
        self.poll_copy(&ctx);
        self.cameras
            .set_live_view(self.project.is_some() && self.tab == Tab::Cinematography);
        let events = self.cameras.poll();
        self.camera_events(events);
        self.apply_shot_camera();

        let mut actions = Vec::new();
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.native_menu {
            menu.update(self.project.is_some(), self.settings.recent());
            actions.extend(menu.poll());
        }
        #[cfg(not(target_os = "macos"))]
        {
            if self.dialog.is_none() && self.copy.is_none() {
                actions.extend(file_menu::shortcuts(&ctx, self.project.is_some()));
            }
            egui::Panel::top("menu_bar")
                .frame(menu_bar_frame())
                .show(ui, |ui| {
                    let has_project = self.project.is_some();
                    actions.extend(file_menu::menu_bar(ui, has_project, self.settings.recent()));
                });
        }

        if self.project.is_some() {
            egui::Panel::top("tab_bar")
                .frame(bar_frame())
                .show(ui, |ui| self.tab_bar(ui));
        }

        egui::Panel::bottom("status_bar")
            .frame(bar_frame())
            .show(ui, |ui| {
                status_bar(ui, self.project.as_ref(), self.cameras.connection());
            });

        if !self.errors.is_empty() {
            egui::Panel::top("errors")
                .frame(
                    egui::Frame::new()
                        .fill(theme::ERROR_BACKGROUND)
                        .inner_margin(egui::Margin::symmetric(12, 8)),
                )
                .show(ui, |ui| self.error_list(ui));
        }

        egui::CentralPanel::no_frame().show(ui, |ui| {
            if self.project.is_some() {
                self.tab_content(ui);
            } else {
                actions.extend(picker::ui(ui, self.settings.recent()));
            }
        });

        self.show_dialog(&ctx);
        if let Some(copy) = &self.copy {
            copying(&ctx, &copy.dest);
        }

        for action in actions {
            self.file_action(&ctx, action);
        }
    }

    fn on_exit(&mut self) {
        self.finish_capture();
        if let Some(project) = &mut self.project {
            self.direct.commit_edits(project, &mut self.errors);
        }
    }
}

// Opening, creating, copying, and closing projects (F3-F9).
impl BrickApp {
    fn file_action(&mut self, ctx: &egui::Context, action: FileAction) {
        // Nothing may change the open project while it is being copied.
        if self.copy.is_some() && action != FileAction::Quit {
            return;
        }
        match action {
            FileAction::New => {
                let location = directories::UserDirs::new().map(|dirs| {
                    dirs.document_dir()
                        .unwrap_or_else(|| dirs.home_dir())
                        .to_owned()
                });
                self.dialog = Some(ProjectDialog::new(DialogKind::New, "", location.as_deref()));
            }
            FileAction::Open => {
                let mut picker = rfd::FileDialog::new().set_title("Open Project");
                if let Some(parent) = self.project.as_ref().and_then(|p| p.root().parent()) {
                    picker = picker.set_directory(parent);
                }
                if let Some(path) = picker.pick_folder() {
                    self.open_project(&path);
                }
            }
            FileAction::OpenRecent(path) => self.open_project(&path),
            FileAction::ClearRecent => {
                if let Err(e) = self.settings.clear_recent() {
                    self.errors
                        .push(format!("Could not save the recent projects: {e}"));
                }
            }
            FileAction::SaveAs => {
                let Some(project) = &self.project else {
                    return;
                };
                self.dialog = Some(ProjectDialog::new(
                    DialogKind::SaveAs,
                    &format!("{} copy", project.name()),
                    project.root().parent(),
                ));
            }
            FileAction::Close => self.close_project(),
            FileAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    /// Opens the project in `path` in place of the open one (F4, F9). The
    /// open project stays open if the new one cannot be opened.
    fn open_project(&mut self, path: &Path) {
        let same = |root: &Path| match (path.canonicalize(), root.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => path == root,
        };
        if self.project.as_ref().is_some_and(|p| same(p.root())) {
            return;
        }
        if !Project::exists(path) {
            self.errors.push(format!(
                "{} is not a Brick project because it has no project.conf.",
                path.display()
            ));
            return;
        }
        match Project::open(path) {
            Ok(project) => {
                self.close_project();
                self.set_project(project);
            }
            Err(e) => self.errors.push(format!("Could not open the project: {e}")),
        }
    }

    /// Makes `project` the open project and remembers it as the most recent
    /// one (R9, P11).
    fn set_project(&mut self, project: Project) {
        if let Err(e) = self.settings.add_recent(project.root(), project.name()) {
            self.errors
                .push(format!("Could not save the recent projects: {e}"));
        }
        self.project = Some(project);
        self.direct = DirectTab::default();
        self.cinematography = CinematographyTab::default();
        self.applied_shot = None;
        self.tab = Tab::Direct;
    }

    /// Closes the open project and returns to the project picker (F8, F9).
    /// Every change is already on disk (P6), so there is nothing to ask.
    fn close_project(&mut self) {
        self.finish_capture();
        if let Some(project) = &mut self.project {
            self.direct.commit_edits(project, &mut self.errors);
        }
        self.project = None;
        self.direct = DirectTab::default();
        self.cinematography = CinematographyTab::default();
        self.pending_settings.clear();
        self.applied_shot = None;
        self.tab = Tab::Direct;
    }

    /// Waits for a running capture and writes it before the project closes
    /// (F9, P7).
    fn finish_capture(&mut self) {
        if self.cameras.is_capturing() {
            let events = self.cameras.finish_capture(CAPTURE_TIMEOUT);
            self.camera_events(events);
        }
        if self.pending_test.take().is_some() {
            self.errors
                .push("The camera did not finish the test shot before the project closed.".into());
        }
    }

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        match dialog.show(ctx) {
            None => self.dialog = Some(dialog),
            Some(DialogResult::Cancel) => {}
            Some(DialogResult::Submit {
                dest,
                name,
                frame_rate,
            }) => match dialog.kind() {
                DialogKind::New => match Project::create(&dest, &name, frame_rate) {
                    Ok(project) => {
                        self.close_project();
                        self.set_project(project);
                    }
                    Err(e) => {
                        dialog.set_error(format!("Could not create the project: {e}"));
                        self.dialog = Some(dialog);
                    }
                },
                DialogKind::SaveAs => self.start_copy(dest, name),
            },
        }
    }

    /// Copies the open project on a background thread so the window keeps
    /// drawing while large RAW files are copied (F6).
    fn start_copy(&mut self, dest: PathBuf, name: String) {
        let Some(project) = &mut self.project else {
            return;
        };
        self.direct.commit_edits(project, &mut self.errors);
        let source = project.root().to_owned();
        let (sender, done) = mpsc::channel();
        let job_dest = dest.clone();
        thread::spawn(move || {
            let _ = sender.send(project::copy_project(&source, &job_dest, &name));
        });
        self.copy = Some(CopyJob { dest, done });
    }

    fn poll_copy(&mut self, ctx: &egui::Context) {
        let Some(copy) = &self.copy else {
            return;
        };
        let result = match copy.done.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(project::Error::Invalid(
                "The copy stopped unexpectedly.".into(),
            )),
        };
        let dest = copy.dest.clone();
        self.copy = None;
        match result {
            Ok(()) => match Project::open(&dest) {
                Ok(project) => {
                    self.close_project();
                    self.set_project(project);
                }
                Err(e) => self
                    .errors
                    .push(format!("Could not open the copied project: {e}")),
            },
            Err(e) => self
                .errors
                .push(format!("Could not save the project as a copy: {e}")),
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.project {
            Some(project) => format!("{} — Brick", project.name()),
            None => "Brick".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl BrickApp {
    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for tab in Tab::ALL {
                let selected = self.tab == tab;
                let text = RichText::new(tab.title()).size(15.0);
                let button = egui::Button::new(text)
                    .min_size(egui::vec2(140.0, 32.0))
                    .fill(if selected {
                        theme::BRICK
                    } else {
                        theme::SURFACE
                    });
                if ui.add(button).clicked() {
                    self.tab = tab;
                }
            }
        });
    }

    fn error_list(&mut self, ui: &mut egui::Ui) {
        let mut dismissed = None;
        for (index, error) in self.errors.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(error);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Dismiss").clicked() {
                        dismissed = Some(index);
                    }
                });
            });
        }
        if let Some(index) = dismissed {
            self.errors.remove(index);
        }
    }

    fn tab_content(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else {
            return;
        };
        match self.tab {
            Tab::Direct => self.direct.ui(ui, project, &mut self.errors),
            Tab::Cinematography => {
                let mut actions = Vec::new();
                self.cinematography
                    .ui(ui, project, &mut self.cameras, &mut actions);
                for action in actions {
                    self.cinematography_action(action);
                }
            }
            tab => {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(tab.summary())
                            .size(16.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            }
        }
    }
}

// Cameras (C5-C7, CT5, CT7).
impl BrickApp {
    fn cinematography_action(&mut self, action: cinematography::Action) {
        let active_key = self.active_shot_key();
        match action {
            cinematography::Action::Connect(id) => {
                self.cameras.connect(&id);
                if let Err(e) = self.settings.set_last_camera(&id) {
                    self.errors
                        .push(format!("Could not save the last used camera: {e}"));
                }
            }
            cinematography::Action::Set(setting, value) => {
                if let Some(key) = active_key {
                    self.cameras.set(setting, &value);
                    self.pending_settings.push((key, setting, value));
                }
            }
            cinematography::Action::TestShot => {
                if active_key.is_some() && self.cameras.capture() {
                    self.pending_test = active_key;
                }
            }
        }
    }

    fn camera_events(&mut self, events: Vec<CameraEvent>) {
        for event in events {
            match event {
                CameraEvent::Connected => {
                    self.applied_shot = None;
                    self.pending_settings.clear();
                }
                CameraEvent::SettingChanged(setting, value) => {
                    let Some(index) = self
                        .pending_settings
                        .iter()
                        .position(|(_, s, v)| *s == setting && *v == value)
                    else {
                        continue;
                    };
                    let (key, _, _) = self.pending_settings.remove(index);
                    self.save_shot_setting(key, setting, value);
                }
                CameraEvent::Captured(capture) => {
                    let key = self.pending_test.take();
                    let target = self
                        .project
                        .as_mut()
                        .and_then(|p| Some((key?, p.find_shot(key?)?, p)));
                    let Some((key, shot, project)) = target else {
                        self.errors.push(
                            "The test shot was not saved because its shot no longer exists.".into(),
                        );
                        continue;
                    };
                    match project.add_test_shot(shot, &capture) {
                        Ok(number) => {
                            if self.applied_shot == Some(key) {
                                self.cinematography.show_test(number);
                            }
                        }
                        Err(e) => self
                            .errors
                            .push(format!("Could not save the test shot: {e}")),
                    }
                }
                CameraEvent::Error(message) => {
                    if !self.cameras.is_capturing() {
                        self.pending_test = None;
                    }
                    self.errors.push(message);
                }
            }
        }
    }

    fn active_shot_key(&self) -> Option<ShotKey> {
        let project = self.project.as_ref()?;
        Some(project.shot(project.active()?.shot_id())?.key())
    }

    fn save_shot_setting(&mut self, key: ShotKey, setting: Setting, value: String) {
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(shot) = project.find_shot(key) else {
            return;
        };
        let Some(mut camera) = project.shot(shot).map(|s| s.camera().clone()) else {
            return;
        };
        camera.set(setting, Some(value));
        if let Err(e) = project.set_shot_camera(shot, &camera) {
            self.errors
                .push(format!("Could not save the camera settings: {e}"));
        }
    }

    /// Sets the camera to the active shot's saved settings whenever the
    /// active shot or the camera changes, so every take of a shot uses the
    /// same exposure (CT7). Settings the shot has not saved yet are taken
    /// from the camera and saved.
    fn apply_shot_camera(&mut self) {
        if !self.cameras.is_connected() {
            return;
        }
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(id) = project.active().map(|take| take.shot_id()) else {
            return;
        };
        let Some(shot) = project.shot(id) else {
            return;
        };
        if self.applied_shot == Some(shot.key()) {
            return;
        }
        self.applied_shot = Some(shot.key());
        let saved = shot.camera().clone();
        let current = self.cameras.settings().clone();
        let mut complete = saved.clone();
        for control in self.cameras.controls() {
            match saved.get(control.setting) {
                Some(value) if current.get(control.setting) != Some(value) => {
                    self.cameras.set(control.setting, value);
                }
                Some(_) => {}
                None => complete.set(
                    control.setting,
                    current.get(control.setting).map(str::to_owned),
                ),
            }
        }
        if complete != saved
            && let Err(e) = project.set_shot_camera(id, &complete)
        {
            self.errors
                .push(format!("Could not save the camera settings: {e}"));
        }
    }
}

/// Shown while Save Project As copies the project (F6).
fn copying(ctx: &egui::Context, dest: &Path) {
    egui::Modal::new(egui::Id::unique("copying_project")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Copying project…").size(17.0).strong());
        });
        ui.add_space(6.0);
        ui.label(RichText::new(dest.display().to_string()).color(theme::TEXT_DIM));
    });
}

fn status_bar(ui: &mut egui::Ui, project: Option<&Project>, camera: &Connection) {
    let active = project.and_then(|p| Some((p, p.active()?)));
    let take = active.and_then(|(p, id)| {
        let scene = p.scene(id.scene)?;
        let shot = p.shot(id.shot_id())?;
        Some((
            format!("{}  /  {}  /  Take {}", scene.name(), shot.name(), id.take),
            p.take(id)?.frames().len(),
        ))
    });
    ui.horizontal(|ui| {
        let camera = match camera {
            Connection::None => "None connected".to_owned(),
            Connection::Connecting(info) => format!("{} (connecting…)", info.name),
            Connection::Connected(info) => format!("{} (connected)", info.name),
            Connection::Lost(info, _) => format!("{} (disconnected)", info.name),
        };
        status_item(ui, "Camera", &camera);
        ui.separator();
        match &take {
            Some((label, _)) => status_item(ui, "Active", label),
            None => status_item(ui, "Active", "No take active"),
        }
        // Right-to-left layouts place widgets in reverse, so add them last first.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let frames = take.as_ref().map_or(0, |(_, frames)| *frames);
            status_item_reversed(ui, "Frames", &frames.to_string());
            if let Some(project) = project {
                ui.separator();
                let fps = format!("{} fps", project.frame_rate().fps());
                status_item_reversed(ui, "Frame rate", &fps);
            }
        });
    });
}

fn status_item(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).color(theme::TEXT_DIM));
    ui.label(value);
}

fn status_item_reversed(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(value);
    ui.label(RichText::new(label).color(theme::TEXT_DIM));
}

#[cfg(not(target_os = "macos"))]
fn menu_bar_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::BACKGROUND)
        .inner_margin(egui::Margin::symmetric(4, 2))
}

fn bar_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

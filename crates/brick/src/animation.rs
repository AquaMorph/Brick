//! The Animation tab: live view with onion skinning, capture, the timeline,
//! frame edits with undo, and playback (W5, AT1-AT13, K1, K2).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Key, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};

use crate::camera::{Cameras, Connection};
use crate::cinematography::{fit, placeholder, spinner};
use crate::direct::{dim, panel_frame, section_title};
use crate::images::{self, Image, Images};
use crate::project::{Preview, Project, ShotKey, TakeId, TrashedImage};
use crate::theme;

/// Size of a frame in the timeline.
const THUMB_SIZE: Vec2 = Vec2::new(120.0, 80.0);
/// Width of the gap between frames, which can be clicked to move the capture
/// point there (AT13) or dropped on to move a frame there (AT10).
const GAP_WIDTH: f32 = 12.0;
/// Height of the timeline panel, with its toolbar and scroll bar.
const TIMELINE_HEIGHT: f32 = THUMB_SIZE.y + 76.0;
/// Longest side of a timeline thumbnail, in pixels.
const THUMB_SIDE: u32 = 256;
/// Longest side of a frame decoded for playback and onion skinning. Playback
/// must keep the project frame rate, so it never uses full resolution frames
/// (AT5).
const PLAY_SIDE: u32 = 1280;
/// The most frames onion skinning can show (AT8).
const MAX_ONION_FRAMES: usize = 5;
/// How much fainter each older onion skin frame is than the one after it.
const ONION_FALLOFF: f32 = 0.6;

/// Requests from the tab that the app carries out.
pub enum Action {
    /// Capture a frame into the active take at this timeline position (AT2,
    /// AT13).
    Capture { at: usize },
}

/// What the viewer shows when nothing is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum View {
    #[default]
    Live,
    /// A captured frame, by timeline position (AT9).
    Frame(usize),
}

/// A keyboard shortcut of the Animation tab (K2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Capture,
    PlayStop,
    Previous,
    Next,
    First,
    Last,
    Live,
    Remove,
    Onion,
    Loop,
    HighRes,
    Undo,
    Redo,
}

impl Command {
    /// The command a key press stands for (K2). Ctrl on Linux and Windows,
    /// Cmd on macOS.
    fn from_key(key: Key, modifiers: egui::Modifiers) -> Option<Command> {
        if modifiers.command && !modifiers.alt {
            return match key {
                Key::Z if modifiers.shift => Some(Command::Redo),
                Key::Z => Some(Command::Undo),
                _ => None,
            };
        }
        if modifiers.ctrl || modifiers.alt || modifiers.mac_cmd {
            return None;
        }
        Some(match key {
            Key::Enter => Command::Capture,
            Key::Space => Command::PlayStop,
            Key::ArrowLeft => Command::Previous,
            Key::ArrowRight => Command::Next,
            Key::Home => Command::First,
            Key::End => Command::Last,
            Key::Escape => Command::Live,
            Key::Delete => Command::Remove,
            // The key labelled delete on a Mac keyboard.
            Key::Backspace if cfg!(target_os = "macos") => Command::Remove,
            Key::O => Command::Onion,
            Key::L => Command::Loop,
            Key::H => Command::HighRes,
            _ => return None,
        })
    }

    /// Whether holding the key down repeats the command.
    fn repeats(self) -> bool {
        matches!(self, Command::Previous | Command::Next)
    }
}

/// One change to a take's frame order, kept so it can be undone (AT11).
struct Edit {
    before: Vec<u32>,
    after: Vec<u32>,
    /// Images moved to the trash by the last time this edit was done or
    /// undone, put back when it is undone or redone.
    trashed: Vec<TrashedImage>,
}

/// The undo and redo history of the active take (AT11).
#[derive(Default)]
struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl History {
    fn push(&mut self, edit: Edit) {
        self.undo.push(edit);
        self.redo.clear();
    }
}

/// Frames playing back, or being decoded so they can (AT6).
struct Playback {
    /// Image numbers in playback order, holds included.
    frames: Vec<u32>,
    /// Timeline position of the first frame.
    first: usize,
    preview: Preview,
    /// The live view frame played after the captured frames (AT4).
    live: Option<egui::TextureHandle>,
    /// When the first frame was shown. `None` while frames are decoding.
    started: Option<Instant>,
}

impl Playback {
    fn len(&self) -> usize {
        self.frames.len() + usize::from(self.live.is_some())
    }
}

/// The position in a clip of `len` frames to show `elapsed` after it
/// started, or `None` once a clip that does not loop has finished (AT6,
/// AT7). Timing comes only from the clock, never from how often the UI
/// draws, so a slow redraw skips a frame instead of slowing playback down.
fn position(elapsed: Duration, fps: u32, len: usize, looping: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let index = (elapsed.as_secs_f64() * f64::from(fps)).floor() as usize;
    if index < len {
        Some(index)
    } else if looping {
        Some(index % len)
    } else {
        None
    }
}

/// How long until the frame after the one at `elapsed` is due.
fn until_next(elapsed: Duration, fps: u32) -> Duration {
    let index = (elapsed.as_secs_f64() * f64::from(fps)).floor();
    Duration::from_secs_f64((index + 1.0) / f64::from(fps)).saturating_sub(elapsed)
}

pub struct AnimationTab {
    /// The take everything below belongs to, by its shot's key and its take
    /// number, so the tab resets when another take becomes active.
    take: Option<(ShotKey, u32)>,
    view: View,
    /// Where the next capture is inserted. `None` is the end (AT13).
    capture_at: Option<usize>,
    onion: bool,
    onion_frames: usize,
    onion_opacity: f32,
    looping: bool,
    limit: bool,
    limit_frames: usize,
    high_res: bool,
    history: History,
    playback: Option<Playback>,
    frames: FrameCache,
    images: Images,
    commands: Vec<Command>,
    /// Scroll the timeline to the frame being shown.
    follow: bool,
}

impl Default for AnimationTab {
    fn default() -> Self {
        Self {
            take: None,
            view: View::Live,
            capture_at: None,
            onion: false,
            onion_frames: 1,
            onion_opacity: 0.4,
            looping: false,
            limit: false,
            limit_frames: 12,
            high_res: false,
            history: History::default(),
            playback: None,
            frames: FrameCache::default(),
            images: Images::default(),
            commands: Vec::new(),
            follow: false,
        }
    }
}

impl AnimationTab {
    /// Reads the tab's keyboard shortcuts before any widget can react to the
    /// keys, so they work without focusing a control (K1). Nothing is read
    /// while a text field has focus, or while a menu is open so its own keys
    /// keep working.
    pub fn read_keys(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() || egui::Popup::is_any_open(ctx) {
            return;
        }
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } = event
                else {
                    return true;
                };
                let Some(command) = Command::from_key(*key, *modifiers) else {
                    return true;
                };
                if !*repeat || command.repeats() {
                    self.commands.push(command);
                }
                false
            });
        });
    }

    /// Stops playback, for example when another tab is shown (F9).
    pub fn stop(&mut self) {
        self.playback = None;
        self.commands.clear();
    }

    /// Records a frame captured into `take` at position `at`, so the capture
    /// can be undone, and moves the capture point after it (AT11, AT13).
    pub fn captured(
        &mut self,
        project: &Project,
        take: TakeId,
        at: usize,
        before: Vec<u32>,
        after: Vec<u32>,
    ) {
        if self.take != take_key(project, take) {
            return;
        }
        self.history.push(Edit {
            before,
            after,
            trashed: Vec::new(),
        });
        if self.capture_at.is_some() {
            self.capture_at = Some(at + 1);
        }
        self.view = View::Live;
        self.follow = true;
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Project,
        cameras: &mut Cameras,
        errors: &mut Vec<String>,
        actions: &mut Vec<Action>,
    ) {
        let Some(take) = project.active() else {
            if self.take.is_some() {
                self.reset(None);
            }
            ui.centered_and_justified(|ui| {
                ui.label(dim("Make a take active on the Direct tab to animate it.").size(16.0));
            });
            return;
        };
        let key = take_key(project, take);
        if key != self.take {
            self.reset(key);
        }
        let ctx = ui.ctx().clone();
        self.frames.receive(&ctx);

        for command in std::mem::take(&mut self.commands) {
            self.command(&ctx, command, project, take, cameras, errors, actions);
        }
        self.clamp(project, take);

        egui::Panel::bottom("animation_timeline")
            .resizable(false)
            .exact_size(TIMELINE_HEIGHT)
            .frame(panel_frame())
            .show(ui, |ui| self.timeline(ui, project, take, cameras, errors));

        egui::CentralPanel::no_frame()
            .frame(panel_frame())
            .show(ui, |ui| {
                self.toolbar(ui, cameras);
                ui.add_space(8.0);
                self.viewer(ui, project, take, cameras);
            });
    }

    /// Forgets everything about the previous take.
    fn reset(&mut self, take: Option<(ShotKey, u32)>) {
        self.take = take;
        self.view = View::Live;
        self.capture_at = None;
        self.history = History::default();
        self.playback = None;
        self.frames = FrameCache::default();
        self.images = Images::default();
    }

    /// Keeps the view and capture point inside the timeline after it changed.
    fn clamp(&mut self, project: &Project, take: TakeId) {
        let len = frame_count(project, take);
        if let View::Frame(i) = self.view
            && i >= len
        {
            self.view = if len == 0 {
                View::Live
            } else {
                View::Frame(len - 1)
            };
        }
        if self.capture_at.is_some_and(|at| at >= len) {
            self.capture_at = None;
        }
    }

    /// Where the next capture goes (AT13).
    fn capture_point(&self, len: usize) -> usize {
        self.capture_at.map_or(len, |at| at.min(len))
    }

    #[allow(clippy::too_many_arguments)]
    fn command(
        &mut self,
        ctx: &egui::Context,
        command: Command,
        project: &mut Project,
        take: TakeId,
        cameras: &mut Cameras,
        errors: &mut Vec<String>,
        actions: &mut Vec<Action>,
    ) {
        let len = frame_count(project, take);
        let point = self.capture_point(len);
        match command {
            Command::Capture => self.capture(project, take, cameras, errors, actions),
            Command::PlayStop => {
                if self.playback.is_some() {
                    self.playback = None;
                } else {
                    self.play(ctx, project, take, cameras);
                }
            }
            Command::Previous => {
                self.playback = None;
                self.view = match self.view {
                    View::Live if point > 0 => View::Frame(point - 1),
                    View::Frame(i) if i > 0 => View::Frame(i - 1),
                    view => view,
                };
                self.follow = true;
            }
            Command::Next => {
                self.playback = None;
                self.view = match self.view {
                    View::Frame(i) if i + 1 < len => View::Frame(i + 1),
                    View::Frame(_) => View::Live,
                    View::Live => View::Live,
                };
                self.follow = true;
            }
            Command::First | Command::Last if len > 0 => {
                self.playback = None;
                self.view = View::Frame(if command == Command::First {
                    0
                } else {
                    len - 1
                });
                self.follow = true;
            }
            Command::First | Command::Last => {}
            Command::Live => {
                self.playback = None;
                self.view = View::Live;
                self.follow = true;
            }
            Command::Remove => {
                if let View::Frame(i) = self.view {
                    self.remove(project, take, i, errors);
                }
            }
            Command::Onion => self.onion = !self.onion,
            Command::Loop => self.looping = !self.looping,
            Command::HighRes => {
                self.high_res = !self.high_res;
                self.playback = None;
            }
            Command::Undo => self.undo(project, take, errors),
            Command::Redo => self.redo(project, take, errors),
        }
    }

    fn capture(
        &mut self,
        project: &Project,
        take: TakeId,
        cameras: &Cameras,
        errors: &mut Vec<String>,
        actions: &mut Vec<Action>,
    ) {
        self.playback = None;
        if !cameras.is_connected() {
            errors.push("Connect a camera on the Cinematography tab to capture frames.".into());
            return;
        }
        if cameras.is_capturing() {
            return;
        }
        self.view = View::Live;
        let at = self.capture_point(frame_count(project, take));
        actions.push(Action::Capture { at });
    }

    // Frame edits (AT10, AT11)

    /// Changes the frame order and records the change so it can be undone.
    fn edit(
        &mut self,
        project: &mut Project,
        take: TakeId,
        errors: &mut Vec<String>,
        change: impl FnOnce(&mut Vec<u32>),
    ) -> bool {
        let Some(before) = project.take(take).map(|t| t.frames().to_vec()) else {
            return false;
        };
        let mut after = before.clone();
        change(&mut after);
        if after == before {
            return false;
        }
        self.playback = None;
        match project.set_frames(take, after.clone()) {
            Ok(trashed) => {
                self.history.push(Edit {
                    before,
                    after,
                    trashed,
                });
                true
            }
            Err(e) => {
                errors.push(format!("Could not change the frames: {e}"));
                false
            }
        }
    }

    /// Removes the frame at `index`, moving its image to the trash unless a
    /// hold still shows it (P8).
    fn remove(
        &mut self,
        project: &mut Project,
        take: TakeId,
        index: usize,
        errors: &mut Vec<String>,
    ) {
        let point = self.capture_point(frame_count(project, take));
        if self.edit(project, take, errors, |frames| {
            if index < frames.len() {
                frames.remove(index);
            }
        }) && self.capture_at.is_some()
            && index < point
        {
            self.capture_at = Some(point - 1);
        }
    }

    /// Holds the frame at `index` one frame longer by repeating its image
    /// (AT10).
    fn hold(
        &mut self,
        project: &mut Project,
        take: TakeId,
        index: usize,
        errors: &mut Vec<String>,
    ) {
        let point = self.capture_point(frame_count(project, take));
        if self.edit(project, take, errors, |frames| {
            if let Some(&number) = frames.get(index) {
                frames.insert(index + 1, number);
            }
        }) {
            if self.capture_at.is_some() && index < point {
                self.capture_at = Some(point + 1);
            }
            self.view = View::Frame(index + 1);
            self.follow = true;
        }
    }

    /// Moves the frame at `from` into the gap before position `gap`.
    fn move_frame(
        &mut self,
        project: &mut Project,
        take: TakeId,
        from: usize,
        gap: usize,
        errors: &mut Vec<String>,
    ) {
        let to = if gap > from { gap - 1 } else { gap };
        let point = self.capture_point(frame_count(project, take));
        if self.edit(project, take, errors, |frames| {
            if from < frames.len() && to < frames.len() {
                let number = frames.remove(from);
                frames.insert(to, number);
            }
        }) {
            if self.capture_at.is_some() {
                if from < point && to >= point {
                    self.capture_at = Some(point - 1);
                } else if from >= point && to < point {
                    self.capture_at = Some(point + 1);
                }
            }
            self.view = View::Frame(to);
            self.follow = true;
        }
    }

    fn undo(&mut self, project: &mut Project, take: TakeId, errors: &mut Vec<String>) {
        let Some(mut edit) = self.history.undo.pop() else {
            return;
        };
        match self.step(project, take, &mut edit, false) {
            Ok(()) => self.history.redo.push(edit),
            Err(e) => {
                errors.push(format!("Could not undo: {e}"));
                self.history.undo.push(edit);
            }
        }
    }

    fn redo(&mut self, project: &mut Project, take: TakeId, errors: &mut Vec<String>) {
        let Some(mut edit) = self.history.redo.pop() else {
            return;
        };
        match self.step(project, take, &mut edit, true) {
            Ok(()) => self.history.undo.push(edit),
            Err(e) => {
                errors.push(format!("Could not redo: {e}"));
                self.history.redo.push(edit);
            }
        }
    }

    /// Undoes or redoes one edit: puts back the images it moved to the trash
    /// and restores the frame order from the other side of it.
    fn step(
        &mut self,
        project: &mut Project,
        take: TakeId,
        edit: &mut Edit,
        forward: bool,
    ) -> Result<(), String> {
        let (from, to) = if forward {
            (&edit.before, &edit.after)
        } else {
            (&edit.after, &edit.before)
        };
        if project.take(take).map(|t| t.frames()) != Some(from.as_slice()) {
            self.history = History::default();
            return Err("the take was changed in a way the history does not know about.".into());
        }
        self.playback = None;
        project
            .restore_images(take, &mut edit.trashed)
            .map_err(|e| e.to_string())?;
        edit.trashed = project
            .set_frames(take, to.clone())
            .map_err(|e| e.to_string())?;
        // Show where the change happened.
        let changed = from.iter().zip(to).position(|(a, b)| a != b);
        let index = changed.unwrap_or(from.len().min(to.len()));
        self.view = if to.is_empty() {
            View::Live
        } else {
            View::Frame(index.min(to.len() - 1))
        };
        self.follow = true;
        Ok(())
    }

    // Playback (AT1, AT3-AT7)

    /// Starts decoding the frames to play. Playback starts once all of them
    /// are in memory (AT6).
    fn play(&mut self, ctx: &egui::Context, project: &Project, take: TakeId, cameras: &Cameras) {
        let Some(all) = project.take(take).map(|t| t.frames()) else {
            return;
        };
        let end = self.capture_point(all.len());
        let first = if self.limit {
            end.saturating_sub(self.limit_frames)
        } else {
            0
        };
        let frames = all[first..end].to_vec();
        let preview = if self.high_res {
            Preview::High
        } else {
            Preview::Low
        };
        // The default playback ends on what the camera sees now, so the next
        // frame can be judged before it is captured (AT4).
        let live = match preview {
            Preview::Low => cameras.live_frame().map(|frame| {
                let size = [frame.width as usize, frame.height as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba);
                ctx.load_texture("playback_live", image, egui::TextureOptions::LINEAR)
            }),
            Preview::High => None,
        };
        if frames.is_empty() && live.is_none() {
            return;
        }
        let wanted: HashSet<FrameKey> =
            frames.iter().map(|n| FrameKey::play(*n, preview)).collect();
        self.frames.retain_playback(&wanted);
        for number in &frames {
            let path = project.frame_path(take, *number, preview);
            self.frames.request(FrameKey::play(*number, preview), path);
        }
        self.playback = Some(Playback {
            frames,
            first,
            preview,
            live,
            started: None,
        });
        self.view = View::Live;
    }

    /// Advances playback by the clock and returns what to show: the playback
    /// position, or `None` when nothing is playing.
    fn advance(&mut self, ctx: &egui::Context, fps: u32) -> Option<usize> {
        let playback = self.playback.as_mut()?;
        let Some(started) = playback.started else {
            let ready = playback
                .frames
                .iter()
                .all(|n| self.frames.is_done(FrameKey::play(*n, playback.preview)));
            if ready {
                playback.started = Some(Instant::now());
                ctx.request_repaint();
            }
            return None;
        };
        let elapsed = started.elapsed();
        match position(elapsed, fps, playback.len(), self.looping) {
            Some(index) => {
                ctx.request_repaint_after(until_next(elapsed, fps));
                Some(index)
            }
            None => {
                self.playback = None;
                self.view = View::Live;
                self.follow = true;
                None
            }
        }
    }

    // Drawing

    /// Playback and onion skin controls and the capture button. Buttons
    /// queue the same commands as their shortcuts.
    fn toolbar(&mut self, ui: &mut egui::Ui, cameras: &Cameras) {
        let mut command = None;
        ui.horizontal(|ui| {
            let playing = self.playback.is_some();
            let label = if playing { "Stop" } else { "Play" };
            let play =
                egui::Button::new(RichText::new(label).strong()).min_size(Vec2::new(86.0, 28.0));
            if ui
                .add(play)
                .on_hover_text("Play or stop  (Space)")
                .clicked()
            {
                command = Some(Command::PlayStop);
            }
            ui.toggle_value(&mut self.looping, "Loop")
                .on_hover_text("Play over and over  (L)");
            ui.toggle_value(&mut self.limit, "Last")
                .on_hover_text("Play only the most recent frames");
            ui.add_enabled(
                self.limit,
                egui::DragValue::new(&mut self.limit_frames)
                    .range(1..=9999)
                    .suffix(" frames"),
            );
            if ui
                .toggle_value(&mut self.high_res, "High resolution")
                .on_hover_text("Play the captured images instead of the live view frames  (H)")
                .changed()
            {
                self.playback = None;
            }

            ui.separator();
            ui.toggle_value(&mut self.onion, "Onion skin")
                .on_hover_text("Show earlier frames over live view  (O)");
            ui.add_enabled_ui(self.onion, |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.onion_frames)
                        .range(0..=MAX_ONION_FRAMES)
                        .suffix(" frames"),
                )
                .on_hover_text("How many earlier frames to show");
                ui.add(
                    egui::Slider::new(&mut self.onion_opacity, 0.05..=1.0)
                        .show_value(false)
                        .text("Opacity"),
                );
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let capturing = cameras.is_capturing();
                let label = if capturing { "Capturing…" } else { "Capture" };
                let button = egui::Button::new(RichText::new(label).strong())
                    .fill(theme::BRICK)
                    .min_size(Vec2::new(120.0, 28.0));
                let response = ui
                    .add_enabled(cameras.is_connected() && !capturing, button)
                    .on_hover_text("Capture a frame at the capture point  (Enter)")
                    .on_disabled_hover_text("Needs a connected camera.");
                if response.clicked() {
                    command = Some(Command::Capture);
                }
            });
        });
        if let Some(command) = command {
            self.commands.push(command);
            ui.ctx().request_repaint();
        }
    }

    fn viewer(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        take: TakeId,
        cameras: &mut Cameras,
    ) {
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        ui.painter().rect_filled(area, 0.0, theme::SURFACE);
        let ctx = ui.ctx().clone();
        let fps = project.frame_rate().fps();
        let frames = project.take(take).map_or(&[][..], |t| t.frames());

        if let Some(index) = self.advance(&ctx, fps) {
            let Some(playback) = &self.playback else {
                return;
            };
            let texture = match playback.frames.get(index) {
                Some(number) => self.frames.ready(FrameKey::play(*number, playback.preview)),
                None => playback.live.as_ref(),
            };
            match texture {
                Some(texture) => {
                    ui.painter().image(
                        texture.id(),
                        fit(area, texture.aspect_ratio()),
                        full_uv(),
                        Color32::WHITE,
                    );
                }
                None => placeholder(
                    ui,
                    area,
                    "Missing frame",
                    "This frame's image could not be read.",
                ),
            }
            let label = match playback.frames.get(index) {
                Some(_) => format!("PLAY  ·  {}  ·  {fps} fps", playback.first + index + 1),
                None => format!("PLAY  ·  LIVE  ·  {fps} fps"),
            };
            tag(ui.painter(), area, &label, theme::BRICK);
            return;
        }

        if let Some(playback) = &self.playback {
            // Decoding before playback starts (AT6).
            let done = playback
                .frames
                .iter()
                .filter(|n| self.frames.is_done(FrameKey::play(**n, playback.preview)))
                .count();
            let total = playback.frames.len();
            let rect = Rect::from_center_size(area.center(), Vec2::new(320.0, 60.0));
            ui.painter().rect_filled(rect, 0.0, theme::BACKGROUND);
            ui.put(
                rect.shrink(12.0),
                egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                    .text(format!("Preparing playback  {done} / {total}")),
            );
            return;
        }

        match self.view {
            View::Live => self.live(ui, area, project, take, frames, cameras),
            View::Frame(index) => {
                let Some(&number) = frames.get(index) else {
                    return;
                };
                let path = project.frame_path(take, number, Preview::High);
                match self.images.full(&ctx, &path) {
                    Image::Ready(texture) => {
                        let texture = texture.clone();
                        ui.painter().image(
                            texture.id(),
                            fit(area, texture.aspect_ratio()),
                            full_uv(),
                            Color32::WHITE,
                        );
                    }
                    Image::Failed(e) => {
                        placeholder(ui, area, &format!("Could not show frame {}", index + 1), e)
                    }
                    Image::Loading => {
                        // The live view frame stands in until the full image
                        // is decoded.
                        let low = FrameKey::play(number, Preview::Low);
                        self.frames
                            .request(low, project.frame_path(take, number, Preview::Low));
                        match self.frames.ready(low) {
                            Some(texture) => {
                                ui.painter().image(
                                    texture.id(),
                                    fit(area, texture.aspect_ratio()),
                                    full_uv(),
                                    Color32::WHITE,
                                );
                            }
                            None => spinner(ui, area),
                        }
                    }
                }
                let mut label = format!("FRAME {} / {}", index + 1, frames.len());
                if index > 0 && frames[index - 1] == number {
                    label.push_str("  ·  HOLD");
                }
                tag(ui.painter(), area, &label, Color32::from_black_alpha(200));
            }
        }
    }

    /// Live view with the earlier frames onion skinned over it (AT8).
    fn live(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        project: &Project,
        take: TakeId,
        frames: &[u32],
        cameras: &mut Cameras,
    ) {
        let ctx = ui.ctx().clone();
        let Some(texture) = cameras.live_texture(&ctx) else {
            match cameras.connection() {
                Connection::Connecting(_) => spinner(ui, area),
                Connection::Lost(info, reason) => placeholder(
                    ui,
                    area,
                    &format!("{} is disconnected", info.name),
                    &format!("{reason} Brick reconnects when the camera is available."),
                ),
                Connection::None | Connection::Connected(_) => placeholder(
                    ui,
                    area,
                    "No live view",
                    "Choose a camera on the Cinematography tab.",
                ),
            }
            return;
        };
        let texture = texture.clone();
        let frame = fit(area, texture.aspect_ratio());
        let painter = ui.painter();
        painter.image(texture.id(), frame, full_uv(), Color32::WHITE);

        let point = self.capture_point(frames.len());
        if self.onion && self.onion_frames > 0 {
            let earlier = &frames[point.saturating_sub(self.onion_frames)..point];
            // The oldest first, so the newest is drawn on top.
            for (age, number) in earlier.iter().rev().enumerate().rev() {
                let key = FrameKey::play(*number, Preview::Low);
                self.frames
                    .request(key, project.frame_path(take, *number, Preview::Low));
                if let Some(skin) = self.frames.ready(key) {
                    let alpha = self.onion_opacity * ONION_FALLOFF.powi(age as i32);
                    let tint = Color32::from_white_alpha((alpha * 255.0).round() as u8);
                    painter.image(skin.id(), frame, full_uv(), tint);
                }
            }
        }

        let mut label = "LIVE".to_owned();
        if cameras.is_capturing() {
            label.push_str("  ·  CAPTURING");
        }
        if self.capture_at.is_some() {
            label.push_str(&format!("  ·  INSERTS AT {}", point + 1));
        }
        tag(painter, area, &label, theme::BRICK);
    }

    /// Thumbnails of every frame, the capture point, and the frame edit
    /// controls (AT9, AT10, AT11, AT13).
    fn timeline(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Project,
        take: TakeId,
        cameras: &mut Cameras,
        errors: &mut Vec<String>,
    ) {
        let frames = project
            .take(take)
            .map_or_else(Vec::new, |t| t.frames().to_vec());
        let fps = project.frame_rate().fps();
        let point = self.capture_point(frames.len());
        let selected = match self.view {
            View::Frame(i) if self.playback.is_none() => Some(i),
            _ => None,
        };

        ui.horizontal(|ui| {
            ui.label(section_title("Timeline"));
            ui.label(dim(format!(
                "{} frames  ·  {:.1} s",
                frames.len(),
                frames.len() as f64 / f64::from(fps)
            )));
            ui.separator();
            let undo = ui
                .add_enabled(!self.history.undo.is_empty(), egui::Button::new("Undo"))
                .on_hover_text(shortcut_text(ui, "Undo", false));
            if undo.clicked() {
                self.undo(project, take, errors);
            }
            let redo = ui
                .add_enabled(!self.history.redo.is_empty(), egui::Button::new("Redo"))
                .on_hover_text(shortcut_text(ui, "Redo", true));
            if redo.clicked() {
                self.redo(project, take, errors);
            }
            ui.separator();
            ui.add_enabled_ui(selected.is_some(), |ui| {
                let index = selected.unwrap_or(0);
                if ui
                    .button("Hold")
                    .on_hover_text("Show this frame one frame longer without copying the image")
                    .clicked()
                {
                    self.hold(project, take, index, errors);
                }
                if ui
                    .add_enabled(index > 0, egui::Button::new("Move left"))
                    .on_hover_text("Move this frame one place earlier")
                    .clicked()
                {
                    self.move_frame(project, take, index, index - 1, errors);
                }
                if ui
                    .add_enabled(index + 1 < frames.len(), egui::Button::new("Move right"))
                    .on_hover_text("Move this frame one place later")
                    .clicked()
                {
                    self.move_frame(project, take, index, index + 2, errors);
                }
                if ui
                    .button("Remove")
                    .on_hover_text("Move this frame to the project trash  (Delete)")
                    .clicked()
                {
                    self.remove(project, take, index, errors);
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.capture_at.is_some() {
                    if ui
                        .button("Capture at end")
                        .on_hover_text("New frames go after the last frame")
                        .clicked()
                    {
                        self.capture_at = None;
                    }
                    ui.label(dim(format!("Capturing before frame {}", point + 1)));
                } else {
                    ui.label(dim("Capturing at the end"));
                }
            });
        });
        ui.add_space(6.0);

        let ctx = ui.ctx().clone();
        let playing = self.playback.as_ref().and_then(|p| {
            let index = position(p.started?.elapsed(), fps, p.len(), self.looping)?;
            Some(if index < p.frames.len() {
                Card::Frame(p.first + index)
            } else {
                Card::Live
            })
        });
        // The timeline keeps the playing frame in sight.
        if playing.is_some() {
            self.follow = true;
        }
        let current = playing.or(match self.view {
            View::Frame(i) => Some(Card::Frame(i)),
            View::Live => Some(Card::Live),
        });

        let mut clicked = None;
        let mut moved = None;
        let mut gap_clicked = None;
        let mut menu = None;
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for gap in 0..=frames.len() {
                        if gap == point {
                            let response = self.live_card(ui, cameras, current == Some(Card::Live));
                            if let Some(from) = response.dnd_release_payload::<usize>() {
                                moved = Some((*from, gap));
                            }
                            if response.clicked() {
                                clicked = Some(Card::Live);
                            }
                        } else {
                            let response = gap_slot(ui);
                            if let Some(from) = response.dnd_release_payload::<usize>() {
                                moved = Some((*from, gap));
                            }
                            if response.clicked() {
                                gap_clicked = Some(gap);
                            }
                        }
                        let Some(&number) = frames.get(gap) else {
                            continue;
                        };
                        let held = gap > 0 && frames[gap - 1] == number;
                        let response = self.thumbnail(
                            ui,
                            &ctx,
                            project,
                            take,
                            gap,
                            number,
                            held,
                            current == Some(Card::Frame(gap)),
                            selected == Some(gap),
                        );
                        response.dnd_set_drag_payload(gap);
                        if let Some(from) = response.dnd_release_payload::<usize>() {
                            // Dropped on a frame: before or after it, whichever
                            // half the pointer is over.
                            let after = response
                                .interact_pointer_pos()
                                .or(ctx.pointer_latest_pos())
                                .is_some_and(|p| p.x > response.rect.center().x);
                            moved = Some((*from, gap + usize::from(after)));
                        }
                        if response.clicked() {
                            clicked = Some(Card::Frame(gap));
                        }
                        response.context_menu(|ui| {
                            if ui.button("Hold frame").clicked() {
                                menu = Some(MenuChoice::Hold(gap));
                            }
                            if ui.button("Capture after this frame").clicked() {
                                menu = Some(MenuChoice::CaptureAfter(gap));
                            }
                            ui.separator();
                            if ui.button("Remove frame").clicked() {
                                menu = Some(MenuChoice::Remove(gap));
                            }
                        });
                    }
                    if frames.is_empty() {
                        ui.add_space(12.0);
                        ui.label(dim("No frames yet. Press Enter to capture the first one."));
                    }
                });
            });
        self.follow = false;

        if let Some((from, gap)) = moved
            && gap != from
            && gap != from + 1
        {
            self.move_frame(project, take, from, gap, errors);
        }
        if let Some(gap) = gap_clicked {
            self.capture_at = (gap < frames.len()).then_some(gap);
            self.view = View::Live;
            self.playback = None;
        }
        match clicked {
            Some(Card::Frame(i)) => {
                self.playback = None;
                self.view = if selected == Some(i) {
                    View::Live
                } else {
                    View::Frame(i)
                };
            }
            Some(Card::Live) => {
                self.playback = None;
                self.view = View::Live;
            }
            None => {}
        }
        match menu {
            Some(MenuChoice::Hold(i)) => self.hold(project, take, i, errors),
            Some(MenuChoice::Remove(i)) => self.remove(project, take, i, errors),
            Some(MenuChoice::CaptureAfter(i)) => {
                self.capture_at = (i + 1 < frames.len()).then_some(i + 1);
                self.view = View::Live;
                self.playback = None;
            }
            None => {}
        }
    }

    /// The live view card at the capture point (AT13).
    fn live_card(
        &mut self,
        ui: &mut egui::Ui,
        cameras: &mut Cameras,
        current: bool,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(THUMB_SIZE + Vec2::new(GAP_WIDTH * 2.0, 0.0), Sense::click());
        let card = rect.shrink2(Vec2::new(GAP_WIDTH, 0.0));
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let dropping = response.dnd_hover_payload::<usize>().is_some();
        let painter = ui.painter_at(rect);
        painter.rect_filled(card, 0.0, theme::BACKGROUND);
        let ctx = ui.ctx().clone();
        if let Some(texture) = cameras.live_texture(&ctx) {
            painter.image(
                texture.id(),
                fit(card, texture.aspect_ratio()),
                full_uv(),
                Color32::WHITE,
            );
        }
        let galley = painter.layout_no_wrap("LIVE".into(), FontId::proportional(11.0), theme::TEXT);
        let label = Rect::from_min_size(
            Pos2::new(card.left(), card.bottom() - galley.size().y - 4.0),
            galley.size() + Vec2::new(8.0, 4.0),
        );
        painter.rect_filled(label, 0.0, theme::BRICK);
        painter.galley(label.min + Vec2::new(4.0, 2.0), galley, theme::TEXT);
        let stroke = if current || dropping {
            Stroke::new(2.0, theme::BRICK_HOVER)
        } else {
            Stroke::new(1.0, theme::BRICK)
        };
        painter.rect_stroke(card, 0.0, stroke, egui::StrokeKind::Inside);
        if self.follow && current {
            response.scroll_to_me(None);
        }
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Live view. The next capture goes here.")
    }

    #[allow(clippy::too_many_arguments)]
    fn thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        project: &Project,
        take: TakeId,
        index: usize,
        number: u32,
        held: bool,
        current: bool,
        selected: bool,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(THUMB_SIZE, Sense::click_and_drag());
        if self.follow && current {
            response.scroll_to_me(None);
        }
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::BACKGROUND);
        let key = FrameKey::thumb(number);
        self.frames
            .request(key, project.frame_path(take, number, Preview::Low));
        match self.frames.slot(key) {
            Some(Slot::Ready(texture)) => {
                let tint = if held {
                    Color32::from_gray(150)
                } else {
                    Color32::WHITE
                };
                painter.image(
                    texture.id(),
                    fit(rect, texture.aspect_ratio()),
                    full_uv(),
                    tint,
                );
            }
            Some(Slot::Failed) => {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Missing",
                    FontId::proportional(12.0),
                    theme::TEXT_DIM,
                );
            }
            _ => {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Loading…",
                    FontId::proportional(12.0),
                    theme::TEXT_DIM,
                );
            }
        }
        let dragging = egui::DragAndDrop::payload::<usize>(ctx).is_some_and(|p| *p == index)
            && response.dragged();
        if dragging {
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(160));
        }

        let text = if held {
            format!("{}  HOLD", index + 1)
        } else {
            (index + 1).to_string()
        };
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), theme::TEXT);
        let label = Rect::from_min_size(
            Pos2::new(rect.left(), rect.bottom() - galley.size().y - 4.0),
            galley.size() + Vec2::new(8.0, 4.0),
        );
        painter.rect_filled(label, 0.0, Color32::from_black_alpha(180));
        painter.galley(label.min + Vec2::new(4.0, 2.0), galley, theme::TEXT);

        // Dropping a frame on this one puts it on the side the pointer is.
        if response.dnd_hover_payload::<usize>().is_some()
            && let Some(pointer) = ctx.pointer_latest_pos()
        {
            let x = if pointer.x > rect.center().x {
                rect.right() - 1.5
            } else {
                rect.left() + 1.5
            };
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(3.0, theme::BRICK_HOVER),
            );
        }

        let stroke = if current || selected {
            Stroke::new(2.0, theme::TEXT)
        } else if response.hovered() {
            Stroke::new(1.0, theme::TEXT_DIM)
        } else {
            Stroke::new(1.0, theme::BORDER)
        };
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!(
                "Frame {}, image {number:06}. Drag to move it, right click for more.",
                index + 1
            ))
    }
}

/// A card in the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Card {
    Frame(usize),
    Live,
}

/// A choice from a frame's context menu.
enum MenuChoice {
    Hold(usize),
    Remove(usize),
    CaptureAfter(usize),
}

/// Identifies a take by its shot's key and its take number, which stay the
/// same while shots are renamed or reordered.
fn take_key(project: &Project, take: TakeId) -> Option<(ShotKey, u32)> {
    Some((project.shot(take.shot_id())?.key(), take.take))
}

fn frame_count(project: &Project, take: TakeId) -> usize {
    project.take(take).map_or(0, |t| t.frames().len())
}

fn full_uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}

/// A tooltip naming the undo or redo shortcut (K2).
fn shortcut_text(ui: &egui::Ui, action: &str, shift: bool) -> String {
    let modifiers = if shift {
        egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT)
    } else {
        egui::Modifiers::COMMAND
    };
    let shortcut = egui::KeyboardShortcut::new(modifiers, Key::Z);
    format!("{action}  ({})", ui.ctx().format_shortcut(&shortcut))
}

/// The gap before a frame. Clicking it moves the capture point there (AT13)
/// and dropping a frame on it moves the frame there (AT10).
fn gap_slot(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(GAP_WIDTH, THUMB_SIZE.y), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter_at(rect);
    let x = rect.center().x;
    let line = |color, width| {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(width, color),
        );
    };
    if response.dnd_hover_payload::<usize>().is_some() {
        line(theme::BRICK_HOVER, 3.0);
    } else if response.hovered() {
        line(theme::TEXT_DIM, 1.0);
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Capture new frames here")
}

/// A label in the top left corner of the viewer.
fn tag(painter: &egui::Painter, area: Rect, text: &str, fill: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), theme::TEXT);
    let rect = Rect::from_min_size(
        area.min + Vec2::splat(8.0),
        galley.size() + Vec2::new(12.0, 6.0),
    );
    painter.rect_filled(rect, 0.0, fill);
    painter.galley(rect.min + Vec2::new(6.0, 3.0), galley, theme::TEXT);
}

/// How large a decoded frame is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Size {
    Thumb,
    Play,
}

impl Size {
    fn max_side(self) -> u32 {
        match self {
            Size::Thumb => THUMB_SIDE,
            Size::Play => PLAY_SIDE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FrameKey {
    number: u32,
    preview: Preview,
    size: Size,
}

impl FrameKey {
    fn thumb(number: u32) -> Self {
        Self {
            number,
            preview: Preview::Low,
            size: Size::Thumb,
        }
    }

    fn play(number: u32, preview: Preview) -> Self {
        Self {
            number,
            preview,
            size: Size::Play,
        }
    }
}

enum Slot {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

type Job = (FrameKey, PathBuf);
type Decoded = (FrameKey, Result<egui::ColorImage, String>);

/// The active take's frames, decoded on a few background threads and kept as
/// textures for the timeline, onion skinning, and playback (AT6).
struct FrameCache {
    slots: HashMap<FrameKey, Slot>,
    jobs: Option<Sender<Job>>,
    sender: Sender<Decoded>,
    receiver: Receiver<Decoded>,
    ctx: Option<egui::Context>,
    /// Tells the decoding threads to skip what is left once the cache is
    /// dropped.
    cancelled: Arc<AtomicBool>,
}

impl Default for FrameCache {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            slots: HashMap::new(),
            jobs: None,
            sender,
            receiver,
            ctx: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Drop for FrameCache {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl FrameCache {
    /// Turns decoded frames into textures. Called once per UI frame.
    fn receive(&mut self, ctx: &egui::Context) {
        self.ctx = Some(ctx.clone());
        for (key, result) in self.receiver.try_iter() {
            // A frame that is no longer wanted is dropped.
            let Some(slot) = self.slots.get_mut(&key) else {
                continue;
            };
            *slot = match result {
                Ok(image) => Slot::Ready(ctx.load_texture(
                    format!("frame_{}_{:?}_{:?}", key.number, key.preview, key.size),
                    image,
                    egui::TextureOptions::LINEAR,
                )),
                Err(_) => Slot::Failed,
            };
        }
    }

    /// Starts decoding a frame unless it is decoded or decoding already.
    fn request(&mut self, key: FrameKey, path: PathBuf) {
        if self.slots.contains_key(&key) {
            return;
        }
        let Some(jobs) = self.jobs() else {
            return;
        };
        if jobs.send((key, path)).is_ok() {
            self.slots.insert(key, Slot::Loading);
        }
    }

    fn slot(&self, key: FrameKey) -> Option<&Slot> {
        self.slots.get(&key)
    }

    fn ready(&self, key: FrameKey) -> Option<&egui::TextureHandle> {
        match self.slots.get(&key)? {
            Slot::Ready(texture) => Some(texture),
            Slot::Loading | Slot::Failed => None,
        }
    }

    /// Whether a frame is decoded, or failed to decode and never will.
    fn is_done(&self, key: FrameKey) -> bool {
        matches!(self.slots.get(&key), Some(Slot::Ready(_) | Slot::Failed))
    }

    /// Frees playback frames that are not in `wanted`, keeping thumbnails.
    fn retain_playback(&mut self, wanted: &HashSet<FrameKey>) {
        self.slots
            .retain(|key, _| key.size == Size::Thumb || wanted.contains(key));
    }

    /// The queue of the decoding threads, started on first use.
    fn jobs(&mut self) -> Option<&Sender<Job>> {
        if self.jobs.is_none() {
            let ctx = self.ctx.clone()?;
            let (jobs, queue) = mpsc::channel::<Job>();
            let queue = Arc::new(Mutex::new(queue));
            let threads = thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 4));
            for _ in 0..threads {
                let queue = Arc::clone(&queue);
                let sender = self.sender.clone();
                let ctx = ctx.clone();
                let cancelled = Arc::clone(&self.cancelled);
                thread::spawn(move || {
                    loop {
                        let job = match queue.lock() {
                            Ok(queue) => queue.recv(),
                            Err(_) => return,
                        };
                        let Ok((key, path)) = job else {
                            return;
                        };
                        if cancelled.load(Ordering::Relaxed) {
                            continue;
                        }
                        let result = images::load(&path, key.size.max_side());
                        if sender.send((key, result)).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                });
            }
            self.jobs = Some(jobs);
        }
        self.jobs.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn playback_follows_the_clock() {
        // 12 fps: a new frame every 83.3 ms.
        assert_eq!(position(ms(0), 12, 3, false), Some(0));
        assert_eq!(position(ms(83), 12, 3, false), Some(0));
        assert_eq!(position(ms(84), 12, 3, false), Some(1));
        assert_eq!(position(ms(249), 12, 3, false), Some(2));
        assert_eq!(position(ms(250), 12, 3, false), None);
        // A late redraw skips frames instead of slowing down (AT1).
        assert_eq!(position(ms(200), 12, 10, false), Some(2));
        assert_eq!(position(ms(10), 12, 0, true), None);
    }

    #[test]
    fn looping_starts_over() {
        assert_eq!(position(ms(250), 12, 3, true), Some(0));
        assert_eq!(position(ms(1000), 24, 5, true), Some(4));
        assert_eq!(position(ms(1000), 30, 30, true), Some(0));
    }

    #[test]
    fn repaints_when_the_next_frame_is_due() {
        assert_eq!(until_next(ms(0), 8), ms(125));
        assert_eq!(until_next(ms(100), 8), ms(25));
        assert_eq!(until_next(ms(125), 8), ms(125));
    }

    #[test]
    fn shortcuts_match_k2() {
        let none = egui::Modifiers::NONE;
        let key = |k| Command::from_key(k, none);
        assert_eq!(key(Key::Enter), Some(Command::Capture));
        assert_eq!(key(Key::Space), Some(Command::PlayStop));
        assert_eq!(key(Key::ArrowLeft), Some(Command::Previous));
        assert_eq!(key(Key::ArrowRight), Some(Command::Next));
        assert_eq!(key(Key::Home), Some(Command::First));
        assert_eq!(key(Key::End), Some(Command::Last));
        assert_eq!(key(Key::Escape), Some(Command::Live));
        assert_eq!(key(Key::Delete), Some(Command::Remove));
        assert_eq!(key(Key::O), Some(Command::Onion));
        assert_eq!(key(Key::L), Some(Command::Loop));
        assert_eq!(key(Key::H), Some(Command::HighRes));
        assert_eq!(key(Key::A), None);
        let command = egui::Modifiers::COMMAND;
        assert_eq!(Command::from_key(Key::Z, command), Some(Command::Undo));
        assert_eq!(
            Command::from_key(Key::Z, command.plus(egui::Modifiers::SHIFT)),
            Some(Command::Redo)
        );
        // File menu shortcuts are left alone (F10).
        assert_eq!(Command::from_key(Key::O, command), None);
        assert_eq!(Command::from_key(Key::Enter, egui::Modifiers::ALT), None);
    }
}

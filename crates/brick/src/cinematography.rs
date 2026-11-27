//! The Cinematography tab: live view, camera settings for the active shot,
//! and test shots (W4, CT1-CT8).

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};

use crate::camera::{Cameras, Connection, Setting};
use crate::direct::{dim, panel_frame, section_title};
use crate::images::{Image, Images};
use crate::project::{Project, ShotId, ShotKey};
use crate::theme;

/// Magnification steps for checking focus (CT3).
const ZOOM_LEVELS: [f32; 4] = [1.0, 2.0, 5.0, 10.0];
/// The zoom a double click jumps to.
const FOCUS_ZOOM: f32 = 5.0;
/// Size of a test shot in the strip along the bottom.
const THUMB_SIZE: Vec2 = Vec2::new(150.0, 100.0);
/// Height of the test shot strip, with its title and scroll bar.
const TEST_STRIP_HEIGHT: f32 = THUMB_SIZE.y + 58.0;
/// Shade over the parts of the frame outside an aspect ratio guide.
const GUIDE_SHADE: Color32 = Color32::from_black_alpha(170);
const OVERLAY: Color32 = Color32::from_rgba_premultiplied(110, 110, 110, 110);

/// What the viewer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum View {
    #[default]
    Live,
    /// A test shot full size (CT6).
    Test(u32),
    /// A test shot beside live view (CT6).
    Compare(u32),
}

impl View {
    fn test(self) -> Option<u32> {
        match self {
            View::Live => None,
            View::Test(n) | View::Compare(n) => Some(n),
        }
    }
}

/// Safe area guides for common aspect ratios (CT4).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Guide {
    FourThree,
    SixteenNine,
    Flat,
    Scope,
}

impl Guide {
    const ALL: [Guide; 4] = [
        Guide::FourThree,
        Guide::SixteenNine,
        Guide::Flat,
        Guide::Scope,
    ];

    fn label(self) -> &'static str {
        match self {
            Guide::FourThree => "4:3",
            Guide::SixteenNine => "16:9",
            Guide::Flat => "1.85:1",
            Guide::Scope => "2.39:1",
        }
    }

    fn ratio(self) -> f32 {
        match self {
            Guide::FourThree => 4.0 / 3.0,
            Guide::SixteenNine => 16.0 / 9.0,
            Guide::Flat => 1.85,
            Guide::Scope => 2.39,
        }
    }
}

/// Requests from the tab that the app carries out.
pub enum Action {
    /// Take a test shot for the active shot (CT5).
    TestShot,
    /// Use the camera with this id (C5).
    Connect(String),
    /// Change a camera setting of the active shot (CT7).
    Set(Setting, String),
}

#[derive(Default)]
pub struct CinematographyTab {
    view: View,
    zoom: f32,
    /// The magnified point, from 0 to 1 across and down the image.
    focus: Pos2,
    thirds: bool,
    center_mark: bool,
    guide: Option<Guide>,
    images: Images,
    /// The shot the test shots belong to, so the view resets when the active
    /// shot changes.
    shot: Option<ShotKey>,
}

impl CinematographyTab {
    /// Shows a test shot that was just taken.
    pub fn show_test(&mut self, number: u32) {
        self.view = View::Test(number);
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        cameras: &mut Cameras,
        actions: &mut Vec<Action>,
    ) {
        if self.zoom < 1.0 {
            self.zoom = 1.0;
            self.focus = Pos2::new(0.5, 0.5);
        }
        let shot = project.active().map(|take| take.shot_id());
        let key = shot.and_then(|id| project.shot(id)).map(|s| s.key());
        if key != self.shot {
            self.shot = key;
            self.view = View::Live;
        }
        let tests = shot
            .and_then(|id| project.shot(id))
            .map_or(&[][..], |s| s.tests());
        if self.view.test().is_some_and(|n| !tests.contains(&n)) {
            self.view = View::Live;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input() {
            self.view = View::Live;
        }

        egui::Panel::right("cinematography_controls")
            .resizable(false)
            .exact_size(300.0)
            .frame(panel_frame())
            .show(ui, |ui| controls(ui, project, cameras, actions));

        egui::Panel::bottom("cinematography_tests")
            .resizable(false)
            .exact_size(TEST_STRIP_HEIGHT)
            .frame(panel_frame())
            .show(ui, |ui| self.test_strip(ui, project, shot, tests));

        egui::CentralPanel::no_frame()
            .frame(panel_frame())
            .show(ui, |ui| {
                self.toolbar(ui);
                ui.add_space(8.0);
                self.viewer(ui, project, shot, cameras);
            });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let test = self.view.test();
            if ui
                .selectable_label(self.view == View::Live, "Live view")
                .clicked()
            {
                self.view = View::Live;
            }
            let compare = ui
                .add_enabled(
                    test.is_some(),
                    egui::Button::selectable(matches!(self.view, View::Compare(_)), "Compare"),
                )
                .on_hover_text("Show the selected test shot beside live view.")
                .on_disabled_hover_text("Select a test shot to compare it with live view.");
            if compare.clicked()
                && let Some(n) = test
            {
                self.view = match self.view {
                    View::Compare(_) => View::Test(n),
                    _ => View::Compare(n),
                };
            }

            ui.separator();
            ui.label(dim("Zoom"));
            for level in ZOOM_LEVELS {
                let label = if level == 1.0 {
                    "Fit".to_owned()
                } else {
                    format!("{level}×")
                };
                if ui.selectable_label(self.zoom == level, label).clicked() {
                    self.zoom = level;
                    self.clamp_focus();
                }
            }

            ui.separator();
            ui.toggle_value(&mut self.thirds, "Thirds");
            ui.toggle_value(&mut self.center_mark, "Center");
            egui::ComboBox::from_id_salt("cinematography_guide")
                .selected_text(self.guide.map_or("No guide", Guide::label))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.guide, None, "No guide");
                    for guide in Guide::ALL {
                        ui.selectable_value(&mut self.guide, Some(guide), guide.label());
                    }
                });
        });
    }

    fn viewer(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        shot: Option<ShotId>,
        cameras: &mut Cameras,
    ) {
        let area = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(area, Sense::click_and_drag());
        ui.painter().rect_filled(area, 0.0, theme::SURFACE);

        let ctx = ui.ctx().clone();
        let test_path = self
            .view
            .test()
            .zip(shot)
            .map(|(n, shot)| project.test_shot_path(shot, n));
        let panes: Vec<(Rect, Pane)> = match self.view {
            View::Live => vec![(area, Pane::Live)],
            View::Test(n) => vec![(area, Pane::Test(n))],
            View::Compare(n) => {
                let gap = 6.0;
                let half = (area.width() - gap) / 2.0;
                let left = Rect::from_min_size(area.min, Vec2::new(half, area.height()));
                let right = Rect::from_min_size(
                    Pos2::new(left.right() + gap, area.top()),
                    Vec2::new(half, area.height()),
                );
                ui.painter().rect_filled(
                    Rect::from_x_y_ranges(left.right()..=right.left(), area.y_range()),
                    0.0,
                    theme::BACKGROUND,
                );
                vec![(left, Pane::Test(n)), (right, Pane::Live)]
            }
        };

        let mut hovered_frame = None;
        for (rect, pane) in panes {
            let shown = match pane {
                Pane::Live => match cameras.live_texture(&ctx) {
                    Some(texture) => Some(self.paint(ui, rect, texture)),
                    None => {
                        live_placeholder(ui, rect, cameras.connection());
                        None
                    }
                },
                Pane::Test(n) => {
                    let path = test_path.as_deref();
                    match path.map(|p| self.images.full(&ctx, p)) {
                        Some(Image::Ready(texture)) => {
                            let texture = texture.clone();
                            Some(self.paint(ui, rect, &texture))
                        }
                        Some(Image::Failed(e)) => {
                            placeholder(ui, rect, &format!("Could not show test shot {n}"), e);
                            None
                        }
                        Some(Image::Loading) | None => {
                            spinner(ui, rect);
                            None
                        }
                    }
                }
            };
            if let Some(frame) = shown {
                self.overlays(ui.painter(), frame);
                pane_label(ui.painter(), rect, pane, self.zoom);
                if response.hover_pos().is_some_and(|p| frame.contains(p)) {
                    hovered_frame = Some(frame);
                }
            }
        }

        // Double click to magnify a point, drag to move around (CT3).
        if let Some(frame) = hovered_frame
            && response.double_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            if self.zoom == 1.0 {
                self.zoom = FOCUS_ZOOM;
                self.focus = ((pos - frame.min) / frame.size()).to_pos2();
            } else {
                self.zoom = 1.0;
            }
            self.clamp_focus();
        }
        if self.zoom > 1.0 {
            if response.dragged()
                && let Some(frame) = hovered_frame
            {
                self.focus -= response.drag_delta() / frame.size() / self.zoom;
                self.clamp_focus();
            }
            response.on_hover_cursor(egui::CursorIcon::Grab);
        }
    }

    /// Paints an image fitted into `rect` at the current zoom and returns
    /// where it was drawn.
    fn paint(&self, ui: &egui::Ui, rect: Rect, texture: &egui::TextureHandle) -> Rect {
        let frame = fit(rect, texture.aspect_ratio());
        let half = 0.5 / self.zoom;
        let uv = Rect::from_center_size(self.focus, Vec2::splat(half * 2.0));
        ui.painter().image(texture.id(), frame, uv, Color32::WHITE);
        frame
    }

    /// Composition overlays (CT4). They describe the whole frame, so they are
    /// hidden while magnified.
    fn overlays(&self, painter: &egui::Painter, frame: Rect) {
        if self.zoom > 1.0 {
            return;
        }
        let painter = painter.with_clip_rect(frame);
        let mut composed = frame;
        if let Some(guide) = self.guide {
            composed = fit(frame, guide.ratio());
            for shade in [
                Rect::from_x_y_ranges(frame.x_range(), frame.top()..=composed.top()),
                Rect::from_x_y_ranges(frame.x_range(), composed.bottom()..=frame.bottom()),
                Rect::from_x_y_ranges(frame.left()..=composed.left(), composed.y_range()),
                Rect::from_x_y_ranges(composed.right()..=frame.right(), composed.y_range()),
            ] {
                if shade.width() > 0.0 && shade.height() > 0.0 {
                    painter.rect_filled(shade, 0.0, GUIDE_SHADE);
                }
            }
            painter.rect_stroke(
                composed,
                0.0,
                Stroke::new(1.0, OVERLAY),
                egui::StrokeKind::Outside,
            );
        }
        let stroke = Stroke::new(1.0, OVERLAY);
        if self.thirds {
            for i in 1..3 {
                let t = i as f32 / 3.0;
                let x = composed.left() + composed.width() * t;
                let y = composed.top() + composed.height() * t;
                painter.line_segment(
                    [
                        Pos2::new(x, composed.top()),
                        Pos2::new(x, composed.bottom()),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(composed.left(), y),
                        Pos2::new(composed.right(), y),
                    ],
                    stroke,
                );
            }
        }
        if self.center_mark {
            let c = composed.center();
            let arm = 14.0;
            let stroke = Stroke::new(1.5, Color32::from_white_alpha(200));
            painter.line_segment([c - Vec2::X * arm, c + Vec2::X * arm], stroke);
            painter.line_segment([c - Vec2::Y * arm, c + Vec2::Y * arm], stroke);
        }
    }

    fn clamp_focus(&mut self) {
        let half = 0.5 / self.zoom;
        self.focus.x = self.focus.x.clamp(half, 1.0 - half);
        self.focus.y = self.focus.y.clamp(half, 1.0 - half);
    }

    /// The active shot's test shots, oldest first (CT5, CT6).
    fn test_strip(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        shot: Option<ShotId>,
        tests: &[u32],
    ) {
        ui.horizontal(|ui| {
            ui.label(section_title("Test shots"));
            ui.label(dim(tests.len().to_string()).size(11.0));
        });
        ui.add_space(6.0);
        let Some(shot) = shot else {
            ui.allocate_ui(Vec2::new(ui.available_width(), THUMB_SIZE.y), |ui| {
                ui.label(dim(
                    "Make a take active on the Direct tab to see its shot's test shots.",
                ));
            });
            return;
        };
        if tests.is_empty() {
            ui.allocate_ui(Vec2::new(ui.available_width(), THUMB_SIZE.y), |ui| {
                ui.label(dim(
                    "No test shots yet. Test shots are for checking light and exposure; they never become part of a take.",
                ));
            });
            return;
        }
        let ctx = ui.ctx().clone();
        egui::ScrollArea::horizontal()
            .stick_to_right(true)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for &number in tests {
                        let (rect, response) = ui.allocate_exact_size(THUMB_SIZE, Sense::click());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 0.0, theme::BACKGROUND);
                        let path = project.test_shot_path(shot, number);
                        match self.images.thumbnail(&ctx, &path) {
                            Image::Ready(texture) => {
                                let frame = fit(rect, texture.aspect_ratio());
                                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                                painter.image(texture.id(), frame, uv, Color32::WHITE);
                            }
                            Image::Loading => {
                                painter.text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    "Loading…",
                                    FontId::proportional(12.0),
                                    theme::TEXT_DIM,
                                );
                            }
                            Image::Failed(_) => {
                                painter.text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    "Unreadable",
                                    FontId::proportional(12.0),
                                    theme::TEXT_DIM,
                                );
                            }
                        }
                        let label = painter.layout_no_wrap(
                            number.to_string(),
                            FontId::proportional(11.0),
                            theme::TEXT,
                        );
                        let tag = Rect::from_min_size(
                            Pos2::new(rect.left(), rect.bottom() - label.size().y - 4.0),
                            label.size() + Vec2::new(8.0, 4.0),
                        );
                        painter.rect_filled(tag, 0.0, Color32::from_black_alpha(180));
                        painter.galley(tag.min + Vec2::new(4.0, 2.0), label, theme::TEXT);

                        let selected = self.view.test() == Some(number);
                        let stroke = if selected {
                            Stroke::new(2.0, theme::BRICK_HOVER)
                        } else if response.hovered() {
                            Stroke::new(1.0, theme::TEXT_DIM)
                        } else {
                            Stroke::new(1.0, theme::BORDER)
                        };
                        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
                        let response = response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(format!("Test shot {number}"));
                        if response.clicked() {
                            self.view = match self.view {
                                View::Compare(_) => View::Compare(number),
                                _ if selected => View::Live,
                                _ => View::Test(number),
                            };
                        }
                    }
                });
            });
    }
}

#[derive(Clone, Copy)]
enum Pane {
    Live,
    Test(u32),
}

/// The camera picker, settings, and test shot button (C5, CT1, CT5, CT7).
fn controls(ui: &mut egui::Ui, project: &Project, cameras: &Cameras, actions: &mut Vec<Action>) {
    ui.label(section_title("Camera"));
    ui.add_space(4.0);
    let current = match cameras.connection() {
        Connection::None => None,
        Connection::Connecting(info) | Connection::Connected(info) | Connection::Lost(info, _) => {
            Some(info)
        }
    };
    let selected = current.map_or("No camera", |info| info.name.as_str());
    egui::ComboBox::from_id_salt("cinematography_camera")
        .selected_text(selected)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            let mut any = false;
            for info in cameras.available() {
                any = true;
                let is_current = current.is_some_and(|c| c.id == info.id);
                if ui.selectable_label(is_current, &info.name).clicked() && !is_current {
                    actions.push(Action::Connect(info.id.clone()));
                }
            }
            if !any {
                ui.label(dim("No cameras detected"));
            }
        });
    ui.add_space(4.0);
    let (state, color) = match cameras.connection() {
        Connection::None if cameras.available().next().is_none() => {
            ("No cameras detected".to_owned(), theme::TEXT_DIM)
        }
        Connection::None => ("Choose a camera".to_owned(), theme::TEXT_DIM),
        Connection::Connecting(_) => ("Connecting…".to_owned(), theme::TEXT_DIM),
        Connection::Connected(_) => ("Connected".to_owned(), theme::TEXT),
        Connection::Lost(_, reason) => (format!("Disconnected: {reason}"), theme::BRICK_HOVER),
    };
    ui.label(RichText::new(state).color(color));

    ui.add_space(20.0);
    ui.label(section_title("Camera settings"));
    ui.add_space(4.0);
    let active = project.active().map(|take| take.shot_id());
    let active_shot = active.and_then(|id| Some((project.scene(id.scene)?, project.shot(id)?)));
    match active_shot {
        Some((scene, shot)) => {
            ui.label(RichText::new(format!("{}  /  {}", scene.name(), shot.name())).strong());
        }
        None => {
            ui.label(dim("No take active"));
        }
    }
    ui.add_space(8.0);

    let usable = cameras.is_connected() && active_shot.is_some();
    let mut shown = 0;
    for control in cameras.controls() {
        shown += 1;
        let value = cameras.settings().get(control.setting).unwrap_or("—");
        ui.horizontal(|ui| {
            ui.label(dim(control.setting.label()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(usable, |ui| {
                    egui::ComboBox::from_id_salt(("cinematography_setting", control.setting))
                        .selected_text(value)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for choice in &control.choices {
                                if ui.selectable_label(choice == value, choice).clicked()
                                    && choice != value
                                {
                                    actions.push(Action::Set(control.setting, choice.clone()));
                                }
                            }
                        });
                });
            });
        });
    }
    if shown == 0 {
        ui.label(dim("Connect a camera to change its settings."));
    } else if active_shot.is_none() {
        ui.add_space(4.0);
        ui.label(dim(
            "Make a take active on the Direct tab to change settings.",
        ));
    } else {
        ui.add_space(4.0);
        ui.label(dim(
            "Saved with the shot. Every take of this shot uses these settings.",
        ));
    }

    ui.add_space(20.0);
    ui.label(section_title("Test shot"));
    ui.add_space(4.0);
    let capturing = cameras.is_capturing();
    let label = if capturing {
        "Capturing…"
    } else {
        "Take test shot"
    };
    let button = egui::Button::new(RichText::new(label).strong())
        .fill(theme::BRICK)
        .min_size(Vec2::new(ui.available_width(), 36.0));
    if ui
        .add_enabled(usable && !capturing, button)
        .on_disabled_hover_text("Needs a connected camera and an active take.")
        .clicked()
    {
        actions.push(Action::TestShot);
    }
    ui.add_space(4.0);
    ui.label(dim("Saved in the shot's tests folder, never in a take."));
}

/// The largest rect with `aspect` (width / height) centered in `rect`.
fn fit(rect: Rect, aspect: f32) -> Rect {
    let size = if rect.width() / rect.height() > aspect {
        Vec2::new(rect.height() * aspect, rect.height())
    } else {
        Vec2::new(rect.width(), rect.width() / aspect)
    };
    Rect::from_center_size(rect.center(), size)
}

fn pane_label(painter: &egui::Painter, rect: Rect, pane: Pane, zoom: f32) {
    let mut text = match pane {
        Pane::Live => "LIVE".to_owned(),
        Pane::Test(n) => format!("TEST {n}"),
    };
    if zoom > 1.0 {
        text.push_str(&format!("  ·  {zoom}×"));
    }
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), theme::TEXT);
    let tag = Rect::from_min_size(
        rect.min + Vec2::splat(8.0),
        galley.size() + Vec2::new(12.0, 6.0),
    );
    let fill = match pane {
        Pane::Live => theme::BRICK,
        Pane::Test(_) => Color32::from_black_alpha(200),
    };
    painter.rect_filled(tag, 0.0, fill);
    painter.galley(tag.min + Vec2::new(6.0, 3.0), galley, theme::TEXT);
}

fn live_placeholder(ui: &mut egui::Ui, rect: Rect, connection: &Connection) {
    match connection {
        Connection::None => placeholder(
            ui,
            rect,
            "No camera connected",
            "Choose a camera from the list on the right.",
        ),
        Connection::Connecting(_) | Connection::Connected(_) => spinner(ui, rect),
        Connection::Lost(info, reason) => placeholder(
            ui,
            rect,
            &format!("{} is disconnected", info.name),
            &format!("{reason} Brick reconnects when the camera is available."),
        ),
    }
}

fn placeholder(ui: &egui::Ui, rect: Rect, title: &str, detail: &str) {
    let painter = ui.painter_at(rect);
    painter.text(
        rect.center() - Vec2::Y * 12.0,
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(17.0),
        theme::TEXT,
    );
    painter.text(
        rect.center() + Vec2::Y * 14.0,
        Align2::CENTER_CENTER,
        detail,
        FontId::proportional(13.0),
        theme::TEXT_DIM,
    );
}

fn spinner(ui: &mut egui::Ui, rect: Rect) {
    let size = Vec2::splat(28.0);
    ui.put(
        Rect::from_center_size(rect.center(), size),
        egui::Spinner::new().size(28.0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_ratio() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 100.0));
        let frame = fit(rect, 1.5);
        assert_eq!(frame.size(), Vec2::new(150.0, 100.0));
        assert_eq!(frame.center(), rect.center());
        let frame = fit(rect, 4.0);
        assert_eq!(frame.size(), Vec2::new(300.0, 75.0));
    }
}

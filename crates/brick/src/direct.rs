//! The Direct tab: break the film into scenes, shots, and takes and choose
//! the active take (W3, DT1-DT7).

use std::path::Path;

use eframe::egui::{self, Align, Align2, FontId, Layout, Rect, RichText, Sense, Stroke, Vec2};

use crate::project::{self, Project, ShotId, TakeId};
use crate::theme;

/// Width of a take card in the take grid.
const CARD_WIDTH: f32 = 220.0;
/// Height of the text area below a take card's thumbnail.
const CARD_TEXT_HEIGHT: f32 = 54.0;

/// What the details panel is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Selection {
    #[default]
    None,
    Scene(usize),
    Shot(ShotId),
    Take(TakeId),
}

impl Selection {
    fn scene(self) -> Option<usize> {
        match self {
            Selection::None => None,
            Selection::Scene(scene) => Some(scene),
            Selection::Shot(shot) => Some(shot.scene),
            Selection::Take(take) => Some(take.scene),
        }
    }

    fn shot(self) -> Option<ShotId> {
        match self {
            Selection::Shot(shot) => Some(shot),
            Selection::Take(take) => Some(take.shot_id()),
            Selection::None | Selection::Scene(_) => None,
        }
    }

    fn exists(self, project: &Project) -> bool {
        match self {
            Selection::None => true,
            Selection::Scene(scene) => project.scene(scene).is_some(),
            Selection::Shot(shot) => project.shot(shot).is_some(),
            Selection::Take(take) => project.take(take).is_some(),
        }
    }
}

/// Changes requested while drawing, applied once drawing is done.
enum Action {
    Select(Selection),
    CommitEdits,
    AddScene,
    AddShot(usize),
    AddTake(ShotId),
    MoveScene { from: usize, to: usize },
    MoveShot { shot: ShotId, to: usize },
    AskRemove(Selection),
    CancelRemove,
    Remove(Selection),
    SetActive(TakeId),
    SetChosen(ShotId, Option<u32>),
}

#[derive(Default)]
pub struct DirectTab {
    selection: Selection,
    /// The selected scene or shot's name while it is edited. Written to the
    /// project when the field loses focus.
    name: String,
    /// The selection's notes while they are edited (DT3).
    notes: String,
    /// The item waiting for the user to confirm its removal (DT7).
    confirm_remove: Option<Selection>,
    /// Whether the selection has been set up for the open project.
    started: bool,
}

impl DirectTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, project: &mut Project, errors: &mut Vec<String>) {
        if !self.started {
            self.started = true;
            self.selection = project.active().map_or(Selection::None, Selection::Take);
            self.load_edits(project);
        }
        if !self.selection.exists(project) {
            self.selection = Selection::None;
            self.load_edits(project);
        }

        let mut actions = Vec::new();

        egui::Panel::left("direct_tree")
            .resizable(true)
            .default_size(300.0)
            .min_size(240.0)
            .frame(panel_frame())
            .show(ui, |ui| self.tree(ui, project, &mut actions));

        egui::Panel::right("direct_details")
            .resizable(true)
            .default_size(340.0)
            .min_size(280.0)
            .frame(panel_frame())
            .show(ui, |ui| self.details(ui, project, &mut actions));

        egui::CentralPanel::no_frame()
            .frame(panel_frame())
            .show(ui, |ui| self.takes(ui, project, &mut actions));

        if let Some(target) = self.confirm_remove {
            confirm_remove(ui.ctx(), project, target, &mut actions);
        }

        for action in actions {
            self.apply(action, project, errors);
        }
    }

    /// Writes any name or notes still being edited to the project.
    pub fn commit_edits(&mut self, project: &mut Project, errors: &mut Vec<String>) {
        let name = self.name.trim().to_owned();
        let notes = self.notes.clone();
        let (current_name, current_notes) = match self.selection {
            Selection::None => (None, None),
            Selection::Scene(scene) => project
                .scene(scene)
                .map_or((None, None), |s| (Some(s.name()), Some(s.notes()))),
            Selection::Shot(shot) => project
                .shot(shot)
                .map_or((None, None), |s| (Some(s.name()), Some(s.notes()))),
            Selection::Take(take) => (None, project.take(take).map(|t| t.notes())),
        };
        let rename = current_name.is_some_and(|current| current != name);
        let renote = current_notes.is_some_and(|current| current != notes);

        let mut result = Ok(());
        if rename {
            result = match self.selection {
                Selection::Scene(scene) => project.rename_scene(scene, &name),
                Selection::Shot(shot) => project.rename_shot(shot, &name),
                Selection::None | Selection::Take(_) => Ok(()),
            };
        }
        if renote && result.is_ok() {
            result = match self.selection {
                Selection::Scene(scene) => project.set_scene_notes(scene, &notes),
                Selection::Shot(shot) => project.set_shot_notes(shot, &notes),
                Selection::Take(take) => project.set_take_notes(take, &notes),
                Selection::None => Ok(()),
            };
        }
        if let Err(e) = result {
            errors.push(format!("Could not save changes: {e}"));
            self.load_edits(project);
        }
    }

    fn apply(&mut self, action: Action, project: &mut Project, errors: &mut Vec<String>) {
        self.commit_edits(project, errors);
        let result = match action {
            Action::CommitEdits => Ok(()),
            Action::Select(selection) => {
                self.selection = selection;
                Ok(())
            }
            Action::AddScene => {
                let name = format!("Scene {}", project.scenes().len() + 1);
                project
                    .add_scene(&name)
                    .map(|scene| self.selection = Selection::Scene(scene))
            }
            Action::AddShot(scene) => {
                let count = project.scene(scene).map_or(0, |s| s.shots().len());
                let name = format!("Shot {}", count + 1);
                project
                    .add_shot(scene, &name)
                    .map(|shot| self.selection = Selection::Shot(shot))
            }
            Action::AddTake(shot) => project
                .add_take(shot)
                .map(|take| self.selection = Selection::Take(take)),
            Action::MoveScene { from, to } => project
                .move_scene(from, to)
                .map(|()| self.selection = Selection::Scene(to)),
            Action::MoveShot { shot, to } => project.move_shot(shot, to).map(|()| {
                self.selection = Selection::Shot(ShotId {
                    scene: shot.scene,
                    shot: to,
                });
            }),
            Action::AskRemove(target) => {
                self.confirm_remove = Some(target);
                Ok(())
            }
            Action::CancelRemove => {
                self.confirm_remove = None;
                Ok(())
            }
            Action::Remove(target) => {
                self.confirm_remove = None;
                let result = match target {
                    Selection::None => Ok(()),
                    Selection::Scene(scene) => project.remove_scene(scene),
                    Selection::Shot(shot) => project.remove_shot(shot),
                    Selection::Take(take) => project.remove_take(take),
                };
                result.map(|()| {
                    self.selection = match target {
                        Selection::None | Selection::Scene(_) => Selection::None,
                        Selection::Shot(shot) => Selection::Scene(shot.scene),
                        Selection::Take(take) => Selection::Shot(take.shot_id()),
                    };
                })
            }
            Action::SetActive(take) => project.set_active(Some(take)),
            Action::SetChosen(shot, take) => project.set_chosen_take(shot, take),
        };
        if let Err(e) = result {
            errors.push(e.to_string());
        }
        if !self.selection.exists(project) {
            self.selection = Selection::None;
        }
        self.load_edits(project);
    }

    /// Fills the edit buffers from the current selection.
    fn load_edits(&mut self, project: &Project) {
        let (name, notes) = match self.selection {
            Selection::None => ("", ""),
            Selection::Scene(scene) => project
                .scene(scene)
                .map_or(("", ""), |s| (s.name(), s.notes())),
            Selection::Shot(shot) => project
                .shot(shot)
                .map_or(("", ""), |s| (s.name(), s.notes())),
            Selection::Take(take) => project.take(take).map_or(("", ""), |t| ("", t.notes())),
        };
        self.name = name.to_owned();
        self.notes = notes.to_owned();
    }

    // Scene and shot tree

    fn tree(&self, ui: &mut egui::Ui, project: &Project, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.label(section_title("Scenes"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let scene = self.selection.scene();
                let shot_button = ui
                    .add_enabled(scene.is_some(), egui::Button::new("Add shot"))
                    .on_disabled_hover_text("Select a scene to add a shot to it.");
                if shot_button.clicked()
                    && let Some(scene) = scene
                {
                    actions.push(Action::AddShot(scene));
                }
                if ui.button("Add scene").clicked() {
                    actions.push(Action::AddScene);
                }
            });
        });
        ui.add_space(8.0);

        let active = project.active();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let width = ui.available_width();
                for (scene_index, scene) in project.scenes().iter().enumerate() {
                    let selected = self.selection == Selection::Scene(scene_index);
                    let row = egui::Button::selectable(
                        selected,
                        RichText::new(format!("{}   {}", scene_index + 1, scene.name())).strong(),
                    )
                    .right_text(dim(count(scene.shots().len(), "shot")))
                    .truncate()
                    .min_size(Vec2::new(width, 30.0));
                    if ui.add(row).clicked() {
                        actions.push(Action::Select(Selection::Scene(scene_index)));
                    }

                    for (shot_index, shot) in scene.shots().iter().enumerate() {
                        let id = ShotId {
                            scene: scene_index,
                            shot: shot_index,
                        };
                        let selected = self.selection.shot() == Some(id);
                        let is_active = active.is_some_and(|a| a.shot_id() == id);
                        let right = if is_active {
                            let color = if selected {
                                theme::TEXT
                            } else {
                                theme::BRICK_HOVER
                            };
                            RichText::new("ACTIVE").size(11.0).color(color)
                        } else {
                            dim(count(shot.takes().len(), "take"))
                        };
                        let label = format!(
                            "      {}.{}   {}",
                            scene_index + 1,
                            shot_index + 1,
                            shot.name()
                        );
                        let row = egui::Button::selectable(selected, label)
                            .right_text(right)
                            .truncate()
                            .min_size(Vec2::new(width, 28.0));
                        if ui.add(row).clicked() {
                            actions.push(Action::Select(Selection::Shot(id)));
                        }
                    }
                    ui.add_space(6.0);
                }
            });
    }

    // Take grid

    fn takes(&self, ui: &mut egui::Ui, project: &Project, actions: &mut Vec<Action>) {
        let Some(shot_id) = self.selection.shot() else {
            self.takes_placeholder(ui, project, actions);
            return;
        };
        let (Some(scene), Some(shot)) = (project.scene(shot_id.scene), project.shot(shot_id))
        else {
            return;
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(section_title("Takes"));
                ui.label(
                    RichText::new(format!("{}  /  {}", scene.name(), shot.name()))
                        .size(18.0)
                        .strong(),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("New take").clicked() {
                    actions.push(Action::AddTake(shot_id));
                }
            });
        });
        ui.add_space(12.0);

        if shot.takes().is_empty() {
            ui.label(dim("This shot has no takes yet."));
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(12.0);
                    for take in shot.takes().iter().rev() {
                        self.take_card(ui, project, shot_id.take(take.number()), actions);
                    }
                });
            });
    }

    fn takes_placeholder(&self, ui: &mut egui::Ui, project: &Project, actions: &mut Vec<Action>) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.35);
            match self.selection.scene() {
                None if project.scenes().is_empty() => {
                    ui.label(RichText::new("This project has no scenes yet.").size(16.0));
                    ui.add_space(8.0);
                    if ui.button("Add scene").clicked() {
                        actions.push(Action::AddScene);
                    }
                }
                Some(scene) if project.scene(scene).is_some_and(|s| s.shots().is_empty()) => {
                    ui.label(RichText::new("This scene has no shots yet.").size(16.0));
                    ui.add_space(8.0);
                    if ui.button("Add shot").clicked() {
                        actions.push(Action::AddShot(scene));
                    }
                }
                _ => {
                    ui.label(dim("Select a shot to see its takes.").size(16.0));
                }
            }
        });
    }

    /// A card showing a take's thumbnail, frame count, and duration (DT5).
    fn take_card(
        &self,
        ui: &mut egui::Ui,
        project: &Project,
        id: TakeId,
        actions: &mut Vec<Action>,
    ) {
        let Some(take) = project.take(id) else {
            return;
        };
        let thumb_height = (CARD_WIDTH * 9.0 / 16.0).round();
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(CARD_WIDTH, thumb_height + CARD_TEXT_HEIGHT),
            Sense::click(),
        );
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let selected = self.selection == Selection::Take(id);
        let is_active = project.active() == Some(id);
        let is_chosen = project.shot(id.shot_id()).and_then(|s| s.chosen_take()) == Some(id.take);

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::SURFACE);

        let thumb_rect = Rect::from_min_size(rect.min, Vec2::new(CARD_WIDTH, thumb_height));
        painter.rect_filled(thumb_rect, 0.0, theme::BACKGROUND);
        match project.take_thumbnail(id) {
            Some(path) => {
                let image = egui::Image::new(file_uri(&path)).show_loading_spinner(true);
                let size = image
                    .load_and_calc_size(ui, thumb_rect.size())
                    .unwrap_or(thumb_rect.size());
                image.paint_at(ui, Rect::from_center_size(thumb_rect.center(), size));
            }
            None => {
                painter.text(
                    thumb_rect.center(),
                    Align2::CENTER_CENTER,
                    "No frames",
                    FontId::proportional(13.0),
                    theme::TEXT_DIM,
                );
            }
        }

        let text_top = thumb_rect.bottom() + 10.0;
        let left = rect.left() + 10.0;
        painter.text(
            egui::pos2(left, text_top),
            Align2::LEFT_TOP,
            format!("Take {}", id.take),
            FontId::proportional(15.0),
            theme::TEXT,
        );
        let frames = take.frames().len();
        painter.text(
            egui::pos2(left, text_top + 22.0),
            Align2::LEFT_TOP,
            format!(
                "{}  ·  {}",
                count(frames, "frame"),
                format_duration(frames, project.frame_rate().fps())
            ),
            FontId::proportional(13.0),
            theme::TEXT_DIM,
        );

        let mut badge_right = rect.right() - 10.0;
        if is_chosen {
            badge_right = badge(&painter, badge_right, text_top, "CHOSEN", false) - 6.0;
        }
        if is_active {
            badge(&painter, badge_right, text_top, "ACTIVE", true);
        }

        let stroke = if selected {
            Stroke::new(2.0, theme::BRICK_HOVER)
        } else if response.hovered() {
            Stroke::new(1.0, theme::TEXT_DIM)
        } else {
            Stroke::new(1.0, theme::BORDER)
        };
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);

        if response.clicked() {
            actions.push(Action::Select(Selection::Take(id)));
        }
        if response.double_clicked() {
            actions.push(Action::SetActive(id));
        }
        response.context_menu(|ui| {
            if ui
                .add_enabled(!is_active, egui::Button::new("Make active"))
                .clicked()
            {
                actions.push(Action::SetActive(id));
            }
            let chosen_label = if is_chosen {
                "Clear chosen take"
            } else {
                "Mark as chosen take"
            };
            if ui.button(chosen_label).clicked() {
                actions.push(Action::SetChosen(
                    id.shot_id(),
                    (!is_chosen).then_some(id.take),
                ));
            }
            ui.separator();
            if ui.button("Remove take…").clicked() {
                actions.push(Action::AskRemove(Selection::Take(id)));
            }
        });
    }

    // Details panel

    fn details(&mut self, ui: &mut egui::Ui, project: &Project, actions: &mut Vec<Action>) {
        match self.selection {
            Selection::None => {
                ui.label(section_title("Project"));
                ui.label(RichText::new(project.name()).size(18.0).strong());
                ui.add_space(12.0);
                stat(
                    ui,
                    "Frame rate",
                    &format!("{} fps", project.frame_rate().fps()),
                );
                stat(ui, "Scenes", &project.scenes().len().to_string());
                ui.add_space(8.0);
                ui.label(dim("Location"));
                ui.label(project.root().display().to_string());
                ui.add_space(16.0);
                ui.label(dim("Select a scene, shot, or take to edit it."));
            }
            Selection::Scene(index) => {
                let Some(scene) = project.scene(index) else {
                    return;
                };
                ui.label(section_title(&format!("Scene {}", index + 1)));
                self.name_field(ui, actions);
                stat(ui, "Shots", &scene.shots().len().to_string());
                ui.add_space(8.0);
                reorder_buttons(ui, index, project.scenes().len(), |to| {
                    actions.push(Action::MoveScene { from: index, to });
                });
                self.notes_field(ui, actions);
                remove_button(ui, "Remove scene…", Selection::Scene(index), actions);
            }
            Selection::Shot(id) => {
                let Some(shot) = project.shot(id) else {
                    return;
                };
                let shot_count = project.scene(id.scene).map_or(0, |s| s.shots().len());
                ui.label(section_title(&format!(
                    "Shot {}.{}",
                    id.scene + 1,
                    id.shot + 1
                )));
                self.name_field(ui, actions);
                stat(ui, "Takes", &shot.takes().len().to_string());
                let chosen = shot
                    .chosen_take()
                    .map_or("None".to_owned(), |n| format!("Take {n}"));
                stat(ui, "Chosen take", &chosen);
                ui.add_space(8.0);
                reorder_buttons(ui, id.shot, shot_count, |to| {
                    actions.push(Action::MoveShot { shot: id, to });
                });
                self.notes_field(ui, actions);
                remove_button(ui, "Remove shot…", Selection::Shot(id), actions);
            }
            Selection::Take(id) => {
                let (Some(scene), Some(shot), Some(take)) = (
                    project.scene(id.scene),
                    project.shot(id.shot_id()),
                    project.take(id),
                ) else {
                    return;
                };
                ui.label(section_title(&format!("Take {}", id.take)));
                ui.label(
                    RichText::new(format!("{}  /  {}", scene.name(), shot.name()))
                        .size(16.0)
                        .strong(),
                );
                ui.add_space(12.0);
                let frames = take.frames().len();
                stat(ui, "Frames", &frames.to_string());
                stat(
                    ui,
                    "Duration",
                    &format_duration(frames, project.frame_rate().fps()),
                );
                ui.add_space(12.0);

                let is_active = project.active() == Some(id);
                let is_chosen = shot.chosen_take() == Some(id.take);
                let full = Vec2::new(ui.available_width(), 32.0);
                let activate = if is_active {
                    egui::Button::new("Active take")
                } else {
                    egui::Button::new(RichText::new("Make active").strong()).fill(theme::BRICK)
                };
                if ui
                    .add_enabled(!is_active, activate.min_size(full))
                    .on_hover_text("Capture and playback work on the active take.")
                    .clicked()
                {
                    actions.push(Action::SetActive(id));
                }
                let chosen_label = if is_chosen {
                    "Clear chosen take"
                } else {
                    "Mark as chosen take"
                };
                if ui
                    .add(egui::Button::new(chosen_label).min_size(full))
                    .on_hover_text("Export uses the chosen take of each shot.")
                    .clicked()
                {
                    actions.push(Action::SetChosen(
                        id.shot_id(),
                        (!is_chosen).then_some(id.take),
                    ));
                }
                self.notes_field(ui, actions);
                remove_button(ui, "Remove take…", Selection::Take(id), actions);
            }
        }
    }

    fn name_field(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.add_space(4.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.name)
                .char_limit(project::MAX_NAME_LEN)
                .font(FontId::proportional(18.0))
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus() {
            actions.push(Action::CommitEdits);
        }
        ui.add_space(12.0);
    }

    fn notes_field(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.add_space(16.0);
        ui.label(section_title("Notes"));
        let response = ui.add(
            egui::TextEdit::multiline(&mut self.notes)
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus() {
            actions.push(Action::CommitEdits);
        }
    }
}

/// Asks the user to confirm moving an item to the trash (DT7, P8).
fn confirm_remove(
    ctx: &egui::Context,
    project: &Project,
    target: Selection,
    actions: &mut Vec<Action>,
) {
    let (title, body) = match target {
        Selection::None => return,
        Selection::Scene(index) => {
            let Some(scene) = project.scene(index) else {
                return;
            };
            (
                format!("Remove scene “{}”?", scene.name()),
                format!(
                    "The scene, its {}, and all of their takes move to the project trash.",
                    count(scene.shots().len(), "shot")
                ),
            )
        }
        Selection::Shot(id) => {
            let Some(shot) = project.shot(id) else {
                return;
            };
            (
                format!("Remove shot “{}”?", shot.name()),
                format!(
                    "The shot and its {} move to the project trash.",
                    count(shot.takes().len(), "take")
                ),
            )
        }
        Selection::Take(id) => {
            let Some(take) = project.take(id) else {
                return;
            };
            (
                format!("Remove take {}?", id.take),
                format!(
                    "The take and its {} move to the project trash.",
                    count(take.frames().len(), "frame")
                ),
            )
        }
    };

    let modal = egui::Modal::new(egui::Id::unique("direct_confirm_remove")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(RichText::new(title).size(17.0).strong());
        ui.add_space(8.0);
        ui.label(body);
        ui.label(dim("Nothing is deleted until the trash is emptied."));
        ui.add_space(16.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let remove = egui::Button::new(RichText::new("Move to trash").strong())
                .fill(theme::BRICK)
                .min_size(Vec2::new(120.0, 30.0));
            if ui.add(remove).clicked() {
                actions.push(Action::Remove(target));
            }
            if ui
                .add(egui::Button::new("Cancel").min_size(Vec2::new(90.0, 30.0)))
                .clicked()
            {
                actions.push(Action::CancelRemove);
            }
        });
    });
    if modal.should_close() && !actions.iter().any(|a| matches!(a, Action::Remove(_))) {
        actions.push(Action::CancelRemove);
    }
}

fn reorder_buttons(ui: &mut egui::Ui, index: usize, len: usize, mut move_to: impl FnMut(usize)) {
    ui.horizontal(|ui| {
        let half = Vec2::new(
            (ui.available_width() - ui.spacing().item_spacing.x) / 2.0,
            28.0,
        );
        if ui
            .add_enabled(index > 0, egui::Button::new("Move up").min_size(half))
            .clicked()
        {
            move_to(index - 1);
        }
        if ui
            .add_enabled(
                index + 1 < len,
                egui::Button::new("Move down").min_size(half),
            )
            .clicked()
        {
            move_to(index + 1);
        }
    });
}

fn remove_button(ui: &mut egui::Ui, label: &str, target: Selection, actions: &mut Vec<Action>) {
    ui.add_space(16.0);
    let button = egui::Button::new(RichText::new(label).color(theme::BRICK_HOVER))
        .min_size(Vec2::new(ui.available_width(), 28.0));
    if ui.add(button).clicked() {
        actions.push(Action::AskRemove(target));
    }
}

/// Paints a small label ending at `right` and returns its left edge.
fn badge(painter: &egui::Painter, right: f32, top: f32, text: &str, filled: bool) -> f32 {
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        FontId::proportional(10.0),
        if filled {
            theme::TEXT
        } else {
            theme::BRICK_HOVER
        },
    );
    let size = galley.size() + Vec2::new(10.0, 4.0);
    let rect = Rect::from_min_size(egui::pos2(right - size.x, top + 1.0), size);
    if filled {
        painter.rect_filled(rect, 0.0, theme::BRICK);
    } else {
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, theme::BRICK_HOVER),
            egui::StrokeKind::Inside,
        );
    }
    painter.galley(rect.min + Vec2::new(5.0, 2.0), galley, theme::TEXT);
    rect.left()
}

fn stat(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(dim(label));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(value));
    });
}

fn section_title(text: &str) -> RichText {
    RichText::new(text.to_uppercase())
        .size(11.0)
        .color(theme::TEXT_DIM)
        .strong()
}

fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(theme::TEXT_DIM)
}

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::BACKGROUND)
        .inner_margin(egui::Margin::same(14))
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// A take's running time at the project frame rate (DT5).
fn format_duration(frames: usize, fps: u32) -> String {
    let seconds = frames as f64 / f64::from(fps);
    if seconds < 60.0 {
        format!("{seconds:.1} s")
    } else {
        let minutes = (seconds / 60.0).floor();
        format!("{minutes}:{:04.1}", seconds - minutes * 60.0)
    }
}

/// A `file://` URI the egui image loader can read.
fn file_uri(path: &Path) -> String {
    if cfg!(windows) {
        format!("file:///{}", path.display())
    } else {
        format!("file://{}", path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(0, 12), "0.0 s");
        assert_eq!(format_duration(24, 12), "2.0 s");
        assert_eq!(format_duration(1000, 12), "1:23.3");
        assert_eq!(format_duration(30 * 125, 30), "2:05.0");
    }
}

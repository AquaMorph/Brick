//! The dialog that asks for a project name and location, used by New
//! Project (P3, F3) and Save Project As (F6).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Key, Layout, RichText, Vec2};

use crate::project::{FrameRate, MAX_NAME_LEN, project_dir_name};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    New,
    SaveAs,
}

pub enum DialogResult {
    /// Create or copy the project into `dest`, a folder that does not exist
    /// yet.
    Submit {
        dest: PathBuf,
        name: String,
        frame_rate: FrameRate,
    },
    Cancel,
}

pub struct ProjectDialog {
    kind: DialogKind,
    name: String,
    location: String,
    frame_rate: FrameRate,
    error: Option<String>,
}

impl ProjectDialog {
    pub fn new(kind: DialogKind, name: &str, location: Option<&Path>) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            location: location.map_or_else(String::new, |p| p.display().to_string()),
            frame_rate: FrameRate::default(),
            error: None,
        }
    }

    pub fn kind(&self) -> DialogKind {
        self.kind
    }

    /// Shows an error inside the dialog, for example when creating the
    /// project failed.
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<DialogResult> {
        let mut result = None;
        let (title, confirm) = match self.kind {
            DialogKind::New => ("New Project", "Create"),
            DialogKind::SaveAs => ("Save Project As", "Save Copy"),
        };
        let modal = egui::Modal::new(egui::Id::unique("project_dialog")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(RichText::new(title).size(17.0).strong());
            if self.kind == DialogKind::SaveAs {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "The whole project is copied and the copy is opened. \
                         The original is left unchanged.",
                    )
                    .color(theme::TEXT_DIM),
                );
            }
            ui.add_space(12.0);

            ui.label("Name");
            let name = ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .char_limit(MAX_NAME_LEN)
                    .desired_width(f32::INFINITY),
            );
            if ui.memory(|m| m.focused().is_none()) {
                name.request_focus();
            }
            ui.add_space(8.0);

            ui.label("Location");
            ui.horizontal(|ui| {
                let browse_width = 90.0;
                ui.add(
                    egui::TextEdit::singleline(&mut self.location)
                        .desired_width(ui.available_width() - browse_width - 8.0),
                );
                if ui
                    .add(egui::Button::new("Browse…").min_size(Vec2::new(browse_width, 0.0)))
                    .clicked()
                {
                    let mut picker = rfd::FileDialog::new().set_title("Choose a location");
                    if Path::new(&self.location).is_dir() {
                        picker = picker.set_directory(&self.location);
                    }
                    if let Some(folder) = picker.pick_folder() {
                        self.location = folder.display().to_string();
                    }
                }
            });

            if self.kind == DialogKind::New {
                ui.add_space(8.0);
                ui.label("Frame rate");
                egui::ComboBox::from_id_salt("project_dialog_fps")
                    .selected_text(fps_label(self.frame_rate))
                    .show_ui(ui, |ui| {
                        for fps in FrameRate::SUPPORTED {
                            if let Ok(rate) = FrameRate::try_from(fps) {
                                ui.selectable_value(&mut self.frame_rate, rate, fps_label(rate));
                            }
                        }
                    });
            }

            ui.add_space(8.0);
            let dest = self.dest();
            if let Ok(dest) = &dest {
                ui.label(
                    RichText::new(format!("Project folder: {}", dest.display()))
                        .color(theme::TEXT_DIM),
                );
            }
            if let Some(error) = &self.error {
                ui.add_space(4.0);
                ui.label(RichText::new(error).color(theme::BRICK_HOVER));
            }

            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let submit = egui::Button::new(RichText::new(confirm).strong())
                    .fill(theme::BRICK)
                    .min_size(Vec2::new(110.0, 30.0));
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                if ui.add(submit).clicked() || enter {
                    match dest {
                        Ok(dest) => {
                            result = Some(DialogResult::Submit {
                                dest,
                                name: self.name.trim().to_owned(),
                                frame_rate: self.frame_rate,
                            });
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                if ui
                    .add(egui::Button::new("Cancel").min_size(Vec2::new(90.0, 30.0)))
                    .clicked()
                {
                    result = Some(DialogResult::Cancel);
                }
            });
        });
        if result.is_none() && modal.should_close() {
            result = Some(DialogResult::Cancel);
        }
        result
    }

    /// The folder the project will be written to: the name inside the
    /// location. It must not exist yet (F7).
    fn dest(&self) -> Result<PathBuf, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Enter a name for the project.".into());
        }
        let location = Path::new(self.location.trim());
        if location.as_os_str().is_empty() {
            return Err("Choose a location for the project.".into());
        }
        if !location.is_absolute() {
            return Err("The location must be a full path.".into());
        }
        let dest = location.join(project_dir_name(name));
        if dest.exists() {
            return Err(format!("{} already exists.", dest.display()));
        }
        Ok(dest)
    }
}

fn fps_label(rate: FrameRate) -> String {
    format!("{} fps", rate.fps())
}

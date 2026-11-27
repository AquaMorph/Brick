//! The main window: tab bar, active tab, and status bar (U3, W2).

use eframe::egui::{self, Align, Layout, RichText};

use crate::direct::DirectTab;
use crate::project::{self, Project};
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
    /// Errors shown at the top of the window until dismissed (U5).
    errors: Vec<String>,
}

impl BrickApp {
    pub fn new(project: project::Result<Option<Project>>) -> Self {
        let (project, errors) = match project {
            Ok(project) => (project, Vec::new()),
            Err(e) => (None, vec![format!("Could not open the project: {e}")]),
        };
        Self {
            tab: Tab::Direct,
            project,
            direct: DirectTab::default(),
            errors,
        }
    }
}

impl eframe::App for BrickApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tab_bar")
            .frame(bar_frame())
            .show(ui, |ui| self.tab_bar(ui));

        egui::Panel::bottom("status_bar")
            .frame(bar_frame())
            .show(ui, |ui| status_bar(ui, self.project.as_ref()));

        if !self.errors.is_empty() {
            egui::Panel::top("errors")
                .frame(
                    egui::Frame::new()
                        .fill(theme::ERROR_BACKGROUND)
                        .inner_margin(egui::Margin::symmetric(12, 8)),
                )
                .show(ui, |ui| self.error_list(ui));
        }

        egui::CentralPanel::no_frame().show(ui, |ui| self.tab_content(ui));
    }

    fn on_exit(&mut self) {
        if let Some(project) = &mut self.project {
            self.direct.commit_edits(project, &mut self.errors);
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
            no_project(ui);
            return;
        };
        match self.tab {
            Tab::Direct => self.direct.ui(ui, project, &mut self.errors),
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

fn no_project(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.4);
        ui.label(RichText::new("No project open").size(18.0));
        ui.add_space(6.0);
        ui.label(
            RichText::new("Start Brick with a project folder: brick <folder>")
                .color(theme::TEXT_DIM),
        );
    });
}

fn status_bar(ui: &mut egui::Ui, project: Option<&Project>) {
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
        status_item(ui, "Camera", "None connected");
        ui.separator();
        match &take {
            Some((label, _)) => status_item(ui, "Take", label),
            None => status_item(ui, "Take", "No take active"),
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

fn bar_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

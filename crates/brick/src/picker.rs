//! The project picker shown when no project is open (P2).

use eframe::egui::{self, RichText, Vec2};

use crate::file_menu::FileAction;
use crate::settings::RecentProject;
use crate::theme;

pub fn ui(ui: &mut egui::Ui, recent: &[RecentProject]) -> Option<FileAction> {
    let mut action = None;
    let width = 520.0_f32.min(ui.available_width() - 32.0);
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.18);
        ui.label(RichText::new("Brick").size(28.0).strong());
        ui.add_space(24.0);

        ui.allocate_ui(Vec2::new(width, ui.available_height()), |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                let half = Vec2::new((width - ui.spacing().item_spacing.x) / 2.0, 36.0);
                let new = egui::Button::new(RichText::new("New Project…").strong())
                    .fill(theme::BRICK)
                    .min_size(half);
                if ui.add(new).clicked() {
                    action = Some(FileAction::New);
                }
                if ui
                    .add(egui::Button::new("Open Project…").min_size(half))
                    .clicked()
                {
                    action = Some(FileAction::Open);
                }
            });

            ui.add_space(24.0);
            ui.label(RichText::new("Recent projects").color(theme::TEXT_DIM));
            ui.add_space(4.0);
            if recent.is_empty() {
                ui.label(RichText::new("No recent projects").color(theme::TEXT_DIM));
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for project in recent {
                    if let Some(picked) = recent_row(ui, project, width) {
                        action = Some(picked);
                    }
                }
            });
        });
    });
    action
}

fn recent_row(ui: &mut egui::Ui, project: &RecentProject, width: f32) -> Option<FileAction> {
    let exists = project.exists();
    let path = if exists {
        project.path.display().to_string()
    } else {
        format!("{}  (missing)", project.path.display())
    };
    let mut text = egui::text::LayoutJob::default();
    text.append(
        &project.name,
        0.0,
        egui::TextFormat::simple(egui::FontId::proportional(15.0), theme::TEXT),
    );
    text.append(
        &format!("\n{path}"),
        0.0,
        egui::TextFormat::simple(egui::FontId::proportional(12.0), theme::TEXT_DIM),
    );
    let button = egui::Button::new(text)
        .fill(theme::SURFACE)
        .min_size(Vec2::new(width, 44.0));
    let clicked = ui.add_enabled(exists, button).clicked();
    clicked.then(|| FileAction::OpenRecent(project.path.clone()))
}

//! Brick, a stop motion animation program.

mod animation;
mod app;
mod camera;
mod cinematography;
mod direct;
mod file_menu;
mod help;
mod images;
mod picker;
mod project;
mod project_dialog;
mod settings;
mod theme;

use std::path::PathBuf;

use eframe::egui;

use project::{FrameRate, Project};

/// Minimum supported window size (U4).
const MIN_WINDOW_SIZE: egui::Vec2 = egui::vec2(1280.0, 720.0);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Brick")
            .with_app_id("brick")
            .with_inner_size(MIN_WINDOW_SIZE)
            .with_min_inner_size(MIN_WINDOW_SIZE),
        ..Default::default()
    };

    eframe::run_native(
        "Brick",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app::BrickApp::new(&cc.egui_ctx, open_from_args())))
        }),
    )
}

/// Opens the project folder given on the command line, creating a new
/// project there if it does not hold one yet.
fn open_from_args() -> Option<project::Result<Project>> {
    let root = std::env::args_os().nth(1).map(PathBuf::from)?;
    if Project::exists(&root) {
        return Some(Project::open(&root));
    }
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_owned());
    Some(Project::create(&root, &name, FrameRate::default()))
}

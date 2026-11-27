//! Brick, a stop motion animation program.

mod app;
mod theme;

use eframe::egui;

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
            Ok(Box::new(app::BrickApp::default()))
        }),
    )
}

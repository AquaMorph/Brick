//! The black theme with brick red accents (U1, U2).

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

/// Window and panel background.
pub const BACKGROUND: Color32 = Color32::from_rgb(0, 0, 0);
/// Background of raised surfaces such as the tab bar and status bar.
pub const SURFACE: Color32 = Color32::from_rgb(14, 14, 14);
/// Separator lines between regions.
pub const BORDER: Color32 = Color32::from_rgb(38, 38, 38);
/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(222, 222, 222);
/// Secondary text such as labels in the status bar.
pub const TEXT_DIM: Color32 = Color32::from_rgb(130, 130, 130);
/// The accent color, a dark brick red.
pub const BRICK: Color32 = Color32::from_rgb(150, 42, 30);
/// A lighter brick red for hovered accents.
pub const BRICK_HOVER: Color32 = Color32::from_rgb(178, 56, 40);

/// Installs the Brick theme on the context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.faint_bg_color = SURFACE;
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = BRICK_HOVER;
    visuals.selection.bg_fill = BRICK;
    visuals.selection.stroke = Stroke::new(1.0, TEXT);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;

    // Flat widgets: no rounding, no shadows, accent only on interaction.
    let flat = CornerRadius::ZERO;
    let widgets = &mut visuals.widgets;
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.corner_radius = flat;
        state.expansion = 0.0;
    }
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.inactive.bg_fill = SURFACE;
    widgets.inactive.weak_bg_fill = SURFACE;
    widgets.hovered.bg_fill = BRICK_HOVER;
    widgets.hovered.weak_bg_fill = BRICK_HOVER;
    widgets.hovered.bg_stroke = Stroke::NONE;
    widgets.active.bg_fill = BRICK;
    widgets.active.weak_bg_fill = BRICK;
    widgets.active.bg_stroke = Stroke::NONE;

    ctx.set_visuals(visuals);
}

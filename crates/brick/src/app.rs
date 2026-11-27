//! The main window: tab bar, active tab, and status bar (U3, W2).

use eframe::egui::{self, Align, Layout, RichText};

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
}

impl Default for BrickApp {
    fn default() -> Self {
        Self { tab: Tab::Direct }
    }
}

impl eframe::App for BrickApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tab_bar")
            .frame(bar_frame())
            .show(ui, |ui| self.tab_bar(ui));

        egui::Panel::bottom("status_bar")
            .frame(bar_frame())
            .show(ui, status_bar);

        egui::CentralPanel::default().show(ui, |ui| self.tab_content(ui));
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

    fn tab_content(&self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(self.tab.summary())
                    .size(16.0)
                    .color(theme::TEXT_DIM),
            );
        });
    }
}

fn status_bar(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        status_item(ui, "Camera", "None connected");
        ui.separator();
        status_item(ui, "Take", "No take active");
        // Right-to-left layouts place widgets in reverse, so add them last first.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            status_item_reversed(ui, "Frames", "0");
            ui.separator();
            status_item_reversed(ui, "Frame rate", "12 fps");
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

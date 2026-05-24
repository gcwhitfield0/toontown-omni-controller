//! The bottom status bar: current routing mode, the active slot in individual mode,
//! the save/dirty indicator, and a transient status message.

use crate::router::RoutingMode;
use crate::ui::MultiToonApp;

/// Draws the status bar into `ui`.
pub(crate) fn show(app: &mut MultiToonApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(mode_text(app));
        ui.separator();
        show_save_controls(app, ui);
        if let Some(message) = &app.status_message {
            ui.separator();
            ui.label(message);
        }
    });
}

/// Builds the human-readable mode description, including the active slot's label when
/// in individual mode.
fn mode_text(app: &MultiToonApp) -> String {
    match app.mode {
        RoutingMode::MultiToon => "Mode: Multi-toon (all bound slots)".to_string(),
        RoutingMode::Individual { active_slot } => {
            let label = app
                .config
                .slots
                .get(active_slot as usize)
                .map(|slot| slot.label.as_str())
                .unwrap_or("?");
            format!("Mode: Individual → slot #{active_slot} ({label})")
        }
    }
}

/// Renders the dirty/clean indicator and the Save button.
fn show_save_controls(app: &mut MultiToonApp, ui: &mut egui::Ui) {
    if app.is_dirty {
        ui.colored_label(egui::Color32::YELLOW, "● Unsaved changes");
    } else {
        ui.colored_label(egui::Color32::GREEN, "✓ Saved");
    }
    if ui.button("Save config").clicked() {
        app.save();
    }
}

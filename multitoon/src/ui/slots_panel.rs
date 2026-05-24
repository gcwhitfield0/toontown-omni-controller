//! The left panel: one row per slot, each with a draggable **crosshair** you drag
//! onto the target Toontown window to link it (mirroring the reference tool), plus a
//! label editor, the linked-window display, and add/remove and mode-hotkey controls.
//!
//! The crosshair is an egui drag handle. When the drag is released — even over an
//! external window — the app asks the platform which window is under the cursor and
//! links it to the slot. (Drawing a border on the game window is out of scope, so the
//! confirmation appears in-app instead.)

use crate::config::{Slot, WindowTarget, MAX_SLOTS, MIN_SLOTS};
use crate::platform::WindowInfo;
use crate::ui::{key_combo, HighlightIntent, MultiToonApp};

/// Glyph used for the draggable crosshair handle.
const CROSSHAIR: &str = "✛";

/// Draws the entire slots panel into `ui`.
pub(crate) fn show(app: &mut MultiToonApp, ui: &mut egui::Ui) {
    ui.heading("Slots");
    ui.label("Drag a slot's ✛ crosshair onto its Toontown window to link it.");
    show_window_controls(app, ui);
    ui.separator();

    // Snapshot the cached window list so rows can display linked titles while we
    // mutate the slots, and remember any slot whose crosshair was just released.
    let windows = app.windows.clone();
    let can_remove = app.config.slots.len() > MIN_SLOTS;
    let mut slot_to_remove: Option<usize> = None;
    let mut pick_for_slot: Option<usize> = None;
    let mut highlight: Option<HighlightIntent> = None;
    let mut any_changed = false;

    for (index, slot) in app.config.slots.iter_mut().enumerate() {
        ui.group(|ui| {
            let outcome = show_slot_row(ui, index, slot, can_remove, &windows);
            any_changed |= outcome.changed;
            if outcome.crosshair_released {
                pick_for_slot = Some(index);
            }
            if outcome.remove_clicked {
                slot_to_remove = Some(index);
            }
            // The last interacting crosshair wins; only one can be hovered/dragged.
            if outcome.highlight.is_some() {
                highlight = outcome.highlight;
            }
        });
    }

    app.requested_highlight = highlight;
    if let Some(index) = pick_for_slot {
        app.assign_window_under_cursor(index);
    }
    if let Some(index) = slot_to_remove {
        app.config.slots.remove(index);
        app.is_dirty = true;
    }
    if any_changed {
        app.is_dirty = true;
    }

    show_mode_hotkeys(app, ui);
}

/// What one slot row reports back to [`show`] after a frame.
#[derive(Default)]
struct RowOutcome {
    /// A persisted field (label or link) changed.
    changed: bool,
    /// The crosshair drag ended this frame; the caller should resolve the window.
    crosshair_released: bool,
    /// The row's "Remove slot" button was clicked.
    remove_clicked: bool,
    /// What this row's crosshair wants highlighted (hovered link, or drag preview).
    highlight: Option<HighlightIntent>,
}

/// Live interaction state of a crosshair handle for one frame.
#[derive(Default)]
struct CrosshairState {
    /// The drag ended this frame (the moment to resolve the dropped-on window).
    released: bool,
    /// The pointer is resting on the crosshair.
    hovered: bool,
    /// The crosshair is being dragged right now.
    dragged: bool,
}

/// Renders the "Refresh windows" and "Add slot" buttons.
fn show_window_controls(app: &mut MultiToonApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if ui.button("Refresh windows").clicked() {
            app.refresh_windows();
        }
        let can_add = app.config.slots.len() < MAX_SLOTS;
        if ui
            .add_enabled(can_add, egui::Button::new("Add slot"))
            .clicked()
        {
            let next = app.config.slots.len() + 1;
            app.config.slots.push(Slot::new(&format!("Toon {next}")));
            app.is_dirty = true;
        }
    });
}

/// Renders one slot: crosshair handle, label editor, linked-window display, and the
/// unbind / remove buttons.
fn show_slot_row(
    ui: &mut egui::Ui,
    index: usize,
    slot: &mut Slot,
    can_remove: bool,
    windows: &[WindowInfo],
) -> RowOutcome {
    let mut outcome = RowOutcome::default();
    ui.horizontal(|ui| {
        let crosshair = show_crosshair(ui, index);
        outcome.crosshair_released = crosshair.released;
        outcome.highlight = crosshair_highlight(&crosshair, slot);
        ui.label(format!("#{index}"));
        if ui.text_edit_singleline(&mut slot.label).changed() {
            outcome.changed = true;
        }
        outcome.remove_clicked = ui
            .add_enabled(can_remove, egui::Button::new("Remove slot"))
            .clicked();
    });
    ui.horizontal(|ui| {
        ui.label(format!("↳ {}", link_label(slot, windows)));
        if slot.target.is_some() && ui.button("Unlink").clicked() {
            slot.target = None;
            outcome.changed = true;
        }
    });
    outcome
}

/// Decides what a crosshair's current interaction should highlight: the window under
/// the cursor while dragging, or this slot's linked window while merely hovering.
fn crosshair_highlight(crosshair: &CrosshairState, slot: &Slot) -> Option<HighlightIntent> {
    if crosshair.dragged {
        return Some(HighlightIntent::DragPreview {
            label: slot.label.clone(),
        });
    }
    if crosshair.hovered {
        if let Some(target) = &slot.target {
            return Some(HighlightIntent::Linked {
                target: target.clone(),
                label: slot.label.clone(),
            });
        }
    }
    None
}

/// Renders the draggable crosshair for a slot and reports its interaction state.
///
/// Every crosshair shares the same glyph, so the button is wrapped in a per-slot id
/// scope to keep their drag states from colliding.
fn show_crosshair(ui: &mut egui::Ui, index: usize) -> CrosshairState {
    ui.push_id(("crosshair", index), |ui| {
        let handle = ui
            .add(egui::Button::new(CROSSHAIR).sense(egui::Sense::click_and_drag()))
            .on_hover_text("Press and drag onto the target Toontown window, then release.");
        if handle.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        CrosshairState {
            released: handle.drag_stopped(),
            hovered: handle.hovered(),
            dragged: handle.dragged(),
        }
    })
    .inner
}

/// Describes a slot's linked window, preferring the live title and prompting the user
/// to use the crosshair when the slot is empty.
fn link_label(slot: &Slot, windows: &[WindowInfo]) -> String {
    match &slot.target {
        Some(target) => find_title(target, windows)
            .unwrap_or_else(|| "linked (refresh to see title)".to_string()),
        None => "not linked — drag the ✛ onto a window".to_string(),
    }
}

/// Looks up the cached title for a linked target, if it is still in the list.
fn find_title(target: &WindowTarget, windows: &[WindowInfo]) -> Option<String> {
    windows
        .iter()
        .find(|window| &window.target == target)
        .map(|window| window.title.clone())
}

/// Renders the two mode-hotkey dropdowns at the bottom of the panel.
fn show_mode_hotkeys(app: &mut MultiToonApp, ui: &mut egui::Ui) {
    ui.separator();
    ui.label("Mode hotkeys");
    ui.horizontal(|ui| {
        ui.label("Toggle individual:");
        let id = ui.id().with("toggle_hotkey");
        if key_combo(ui, id, &mut app.config.toggle_individual_mode_hotkey) {
            app.is_dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Cycle active slot:");
        let id = ui.id().with("cycle_hotkey");
        if key_combo(ui, id, &mut app.config.cycle_individual_slot_hotkey) {
            app.is_dirty = true;
        }
    });
}

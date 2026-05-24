//! The central panel: the multi-toon binding table. Each binding maps one physical
//! key to a list of `(slot, output key)` deliveries. Used only in multi-toon mode.

use crate::config::{Binding, KeyOutput, SlotIndex};
use crate::key::Key;
use crate::ui::{key_combo, MultiToonApp};

/// Pending structural edits collected during a frame and applied after the read-only
/// iteration finishes, to avoid mutating the binding list while iterating it.
#[derive(Default)]
struct PendingEdits {
    binding_to_remove: Option<usize>,
    output_to_remove: Option<(usize, usize)>,
    add_output_to: Option<usize>,
}

/// Draws the entire bindings panel into `ui`.
pub(crate) fn show(app: &mut MultiToonApp, ui: &mut egui::Ui) {
    ui.heading("Bindings (multi-toon mode)");
    ui.label("Each physical key fans out to one or more slots.");
    if ui.button("Add binding").clicked() {
        add_binding(app);
    }
    ui.separator();

    // Snapshot slot labels so the output dropdowns can render while bindings mutate.
    let slot_labels: Vec<String> = app
        .config
        .slots
        .iter()
        .map(|slot| slot.label.clone())
        .collect();
    let mut pending = PendingEdits::default();
    let mut any_changed = false;

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (binding_index, binding) in app.config.bindings.iter_mut().enumerate() {
            ui.group(|ui| {
                if show_binding(ui, binding_index, binding, &slot_labels, &mut pending) {
                    any_changed = true;
                }
            });
        }
    });

    if apply_pending(app, pending) {
        any_changed = true;
    }
    if any_changed {
        app.is_dirty = true;
    }
    show_duplicate_warning(app, ui);
}

/// Renders one binding: its physical key, each output row, and per-binding controls.
/// Returns `true` if any value changed.
fn show_binding(
    ui: &mut egui::Ui,
    binding_index: usize,
    binding: &mut Binding,
    slot_labels: &[String],
    pending: &mut PendingEdits,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Physical key:");
        let id = ui.id().with(("physical_key", binding_index));
        if key_combo(ui, id, &mut binding.physical_key) {
            changed = true;
        }
        if ui.button("Remove binding").clicked() {
            pending.binding_to_remove = Some(binding_index);
        }
    });

    for (output_index, output) in binding.outputs.iter_mut().enumerate() {
        if show_output_row(
            ui,
            binding_index,
            output_index,
            output,
            slot_labels,
            pending,
        ) {
            changed = true;
        }
    }
    if ui.button("Add output").clicked() {
        pending.add_output_to = Some(binding_index);
    }
    changed
}

/// Renders one `(slot, output key)` output row. Returns `true` if it changed.
fn show_output_row(
    ui: &mut egui::Ui,
    binding_index: usize,
    output_index: usize,
    output: &mut KeyOutput,
    slot_labels: &[String],
    pending: &mut PendingEdits,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("→ slot");
        if show_slot_dropdown(ui, binding_index, output_index, output, slot_labels) {
            changed = true;
        }
        ui.label("sends");
        let id = ui.id().with(("output_key", binding_index, output_index));
        if key_combo(ui, id, &mut output.output_key) {
            changed = true;
        }
        if ui.button("✕").clicked() {
            pending.output_to_remove = Some((binding_index, output_index));
        }
    });
    changed
}

/// Renders the slot-selection dropdown for one output. Returns `true` if it changed.
fn show_slot_dropdown(
    ui: &mut egui::Ui,
    binding_index: usize,
    output_index: usize,
    output: &mut KeyOutput,
    slot_labels: &[String],
) -> bool {
    let mut changed = false;
    let current = slot_labels
        .get(output.slot_index as usize)
        .cloned()
        .unwrap_or_else(|| format!("#{}", output.slot_index));
    egui::ComboBox::from_id_source(("output_slot", binding_index, output_index))
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (slot_index, label) in slot_labels.iter().enumerate() {
                let value = slot_index as SlotIndex;
                if ui
                    .selectable_value(&mut output.slot_index, value, label)
                    .changed()
                {
                    changed = true;
                }
            }
        });
    changed
}

/// Applies the structural edits queued during rendering. Returns `true` if anything
/// was changed.
fn apply_pending(app: &mut MultiToonApp, pending: PendingEdits) -> bool {
    let mut changed = false;
    if let Some(binding_index) = pending.add_output_to {
        if let Some(binding) = app.config.bindings.get_mut(binding_index) {
            binding.outputs.push(KeyOutput {
                slot_index: 0,
                output_key: binding.physical_key,
            });
            changed = true;
        }
    }
    if let Some((binding_index, output_index)) = pending.output_to_remove {
        if let Some(binding) = app.config.bindings.get_mut(binding_index) {
            if output_index < binding.outputs.len() {
                binding.outputs.remove(output_index);
                changed = true;
            }
        }
    }
    if let Some(binding_index) = pending.binding_to_remove {
        if binding_index < app.config.bindings.len() {
            app.config.bindings.remove(binding_index);
            changed = true;
        }
    }
    changed
}

/// Adds a new binding using the first key not already bound, defaulting to one
/// passthrough output on slot 0.
fn add_binding(app: &mut MultiToonApp) {
    let used: Vec<Key> = app.config.bindings.iter().map(|b| b.physical_key).collect();
    let Some(&unused) = Key::all().iter().find(|key| !used.contains(key)) else {
        app.status_message = Some("All keys are already bound".to_string());
        return;
    };
    app.config.bindings.push(Binding {
        physical_key: unused,
        outputs: vec![KeyOutput {
            slot_index: 0,
            output_key: unused,
        }],
    });
    app.is_dirty = true;
}

/// Shows an inline error when the same physical key is bound more than once, so the
/// user knows a save will be rejected.
fn show_duplicate_warning(app: &MultiToonApp, ui: &mut egui::Ui) {
    if let Some(duplicate) = app.config.duplicate_binding_key() {
        ui.separator();
        ui.colored_label(
            egui::Color32::RED,
            format!("Key {duplicate} is bound more than once — saving is blocked until fixed."),
        );
    }
}

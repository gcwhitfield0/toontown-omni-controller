//! The eframe application: owns all runtime state, captures focused-window key
//! events, and lays out the slots / bindings / status regions.
//!
//! The actual keystroke fan-out happens in [`MultiToonApp::dispatch`], which calls
//! [`crate::router::route`] and then the active [`crate::platform::Platform`].

mod bindings_panel;
mod slots_panel;
mod status_bar;

use crate::config::{Config, SlotIndex, WindowTarget, MAX_SLOTS, MIN_SLOTS};
use crate::key::Key;
use crate::platform::{Highlight, Platform, WindowInfo};
use crate::router::{self, RoutingMode};
use std::path::PathBuf;

/// What the slots panel wants highlighted this frame, before the window is resolved.
pub(crate) enum HighlightIntent {
    /// Frame a slot's already-linked window (crosshair hovered).
    Linked {
        /// The linked window to outline.
        target: WindowTarget,
        /// Label to show in the bubble.
        label: String,
    },
    /// Frame whatever window is under the cursor right now (crosshair being dragged).
    DragPreview {
        /// Label to show in the bubble.
        label: String,
    },
}

/// All runtime state for the controller: configuration, the platform backend, the
/// current routing mode, and the cached window list shown in the picker.
pub struct MultiToonApp {
    /// The persisted configuration currently being edited.
    pub(crate) config: Config,
    /// Where the config is saved.
    pub(crate) config_path: PathBuf,
    /// The OS backend used to enumerate windows and inject keys.
    pub(crate) platform: Box<dyn Platform>,
    /// How keystrokes are currently routed.
    pub(crate) mode: RoutingMode,
    /// Windows from the most recent "Refresh windows" click; not refreshed per frame.
    pub(crate) windows: Vec<WindowInfo>,
    /// Whether the config has unsaved edits.
    pub(crate) is_dirty: bool,
    /// A short transient message shown in the status bar (errors, save confirmations).
    pub(crate) status_message: Option<String>,
    /// What to highlight this frame, set by the slots panel and applied after layout.
    pub(crate) requested_highlight: Option<HighlightIntent>,
    /// Desired toon count for the "Quick setup" generator (edited in the slots panel).
    pub(crate) quick_setup_count: usize,
}

impl MultiToonApp {
    /// Builds the app from a loaded config, its path, and a platform backend,
    /// normalizing the slot count into the supported range.
    pub fn new(mut config: Config, config_path: PathBuf, platform: Box<dyn Platform>) -> Self {
        normalize_slot_count(&mut config);
        let quick_setup_count = config.slots.len();
        MultiToonApp {
            config,
            config_path,
            platform,
            mode: RoutingMode::MultiToon,
            windows: Vec::new(),
            is_dirty: false,
            status_message: None,
            requested_highlight: None,
            quick_setup_count,
        }
    }

    /// Applies the highlight the slots panel requested this frame: resolves the target
    /// window (for drag previews) and asks the platform to draw or clear the overlay.
    fn apply_highlight(&mut self) {
        let highlight = match self.requested_highlight.take() {
            Some(HighlightIntent::Linked { target, label }) => Some(Highlight { target, label }),
            Some(HighlightIntent::DragPreview { label }) => {
                match self.platform.window_at_cursor() {
                    Ok(Some(window)) => Some(Highlight {
                        target: window.target,
                        label,
                    }),
                    Ok(None) => None,
                    Err(error) => {
                        eprintln!("multitoon: highlight pick failed: {error:#}");
                        None
                    }
                }
            }
            None => None,
        };
        if let Err(error) = self.platform.set_highlight(highlight) {
            eprintln!("multitoon: set_highlight failed: {error:#}");
        }
    }

    /// Re-enumerates target windows, replacing the cached list.
    pub(crate) fn refresh_windows(&mut self) {
        match self.platform.list_target_windows() {
            Ok(windows) => {
                let count = windows.len();
                self.windows = windows;
                self.status_message = Some(format!("Found {count} Toontown window(s)"));
            }
            Err(error) => {
                eprintln!("multitoon: window enumeration failed: {error:#}");
                self.status_message = Some(format!("Enumeration error: {error}"));
            }
        }
    }

    /// Assigns the Toontown window currently under the OS cursor to `slot_index`.
    ///
    /// Called when a slot's crosshair drag is released; the released-over window is
    /// resolved by the platform. The resolved window is also cached so its title
    /// shows in the slot row without a separate refresh.
    pub(crate) fn assign_window_under_cursor(&mut self, slot_index: usize) {
        match self.platform.window_at_cursor() {
            Ok(Some(window)) => {
                if !self
                    .windows
                    .iter()
                    .any(|known| known.target == window.target)
                {
                    self.windows.push(window.clone());
                }
                if let Some(slot) = self.config.slots.get_mut(slot_index) {
                    slot.target = Some(window.target.clone());
                    self.is_dirty = true;
                    self.status_message = Some(format!("Linked \"{}\"", window.title));
                }
            }
            Ok(None) => {
                self.status_message = Some("No Toontown window under the cursor".to_string());
            }
            Err(error) => {
                eprintln!("multitoon: window pick failed: {error:#}");
                self.status_message = Some(format!("Pick error: {error}"));
            }
        }
    }

    /// Validates and persists the config, refusing to save while a physical key is
    /// bound more than once.
    pub(crate) fn save(&mut self) {
        if let Some(duplicate) = self.config.duplicate_binding_key() {
            self.status_message = Some(format!("Cannot save: key {duplicate} is bound twice"));
            return;
        }
        match self.config.save(&self.config_path) {
            Ok(()) => {
                self.is_dirty = false;
                self.status_message = Some("Config saved".to_string());
            }
            Err(error) => {
                eprintln!("multitoon: save failed: {error:#}");
                self.status_message = Some(format!("Save error: {error}"));
            }
        }
    }

    /// Reads this frame's key events and turns them into mode changes or dispatches.
    fn handle_input(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed,
                repeat,
                ..
            } = event
            else {
                continue;
            };
            let Some(mapped) = Key::from_egui(key) else {
                continue;
            };
            // Hotkeys are consumed entirely — they never reach a game window.
            if mapped == self.config.toggle_individual_mode_hotkey {
                if pressed && !repeat {
                    self.toggle_mode();
                }
                continue;
            }
            if mapped == self.config.cycle_individual_slot_hotkey {
                if pressed && !repeat {
                    self.cycle_active_slot();
                }
                continue;
            }
            self.dispatch(mapped, pressed);
        }
    }

    /// Flips between multi-toon and individual control, starting individual mode on
    /// the first slot.
    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            RoutingMode::MultiToon => RoutingMode::Individual { active_slot: 0 },
            RoutingMode::Individual { .. } => RoutingMode::MultiToon,
        };
    }

    /// In individual mode, advances the active slot, wrapping around the slot list.
    fn cycle_active_slot(&mut self) {
        if let RoutingMode::Individual { active_slot } = &mut self.mode {
            let slot_count = self.config.slots.len() as SlotIndex;
            *active_slot = (*active_slot + 1) % slot_count;
        }
    }

    /// Routes one physical key to its slots and injects it into each slot's window.
    ///
    /// A missing target or a single failing send is logged and skipped so the other
    /// targets still receive the keystroke.
    fn dispatch(&mut self, physical_key: Key, pressed: bool) {
        let dispatches = router::route(&self.mode, &self.config.bindings, physical_key);
        for dispatch in dispatches {
            let target = self
                .config
                .slots
                .get(dispatch.slot_index as usize)
                .and_then(|slot| slot.target.clone());
            let Some(target) = target else {
                continue; // slot has no window assigned this session
            };
            let result = if pressed {
                self.platform.send_key_down(&target, dispatch.output_key)
            } else {
                self.platform.send_key_up(&target, dispatch.output_key)
            };
            if let Err(error) = result {
                eprintln!(
                    "multitoon: send to slot {} failed: {error:#}",
                    dispatch.slot_index
                );
                self.status_message = Some(format!("Send error: {error}"));
            }
        }
    }
}

impl eframe::App for MultiToonApp {
    /// Per-frame entry point: capture input, then draw the three UI regions.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_input(ctx);

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            status_bar::show(self, ui);
        });
        egui::SidePanel::left("slots_panel")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                slots_panel::show(self, ui);
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            bindings_panel::show(self, ui);
        });

        // The slots panel recorded what (if anything) to highlight; apply it now that
        // layout is done.
        self.apply_highlight();
    }
}

/// Renders a dropdown for choosing a [`Key`], writing the selection into `current`.
/// Returns `true` if the user changed the value this frame.
pub(crate) fn key_combo(ui: &mut egui::Ui, id: egui::Id, current: &mut Key) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_source(id)
        .selected_text(current.to_string())
        .show_ui(ui, |ui| {
            for &candidate in Key::all() {
                if ui
                    .selectable_value(current, candidate, candidate.to_string())
                    .changed()
                {
                    changed = true;
                }
            }
        });
    changed
}

/// Pads or truncates the slot list so its length lands within `MIN_SLOTS..=MAX_SLOTS`.
fn normalize_slot_count(config: &mut Config) {
    use crate::config::Slot;
    while config.slots.len() < MIN_SLOTS {
        let next = config.slots.len() + 1;
        config.slots.push(Slot::new(&format!("Toon {next}")));
    }
    config.slots.truncate(MAX_SLOTS);
}

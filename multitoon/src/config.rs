//! The persisted data model and its JSON load/save.
//!
//! Slot labels, key bindings, and the two mode hotkeys persist between sessions.
//! Window targets do **not**: native window handles are reassigned every launch, so
//! the [`Slot::target`] field is `#[serde(skip)]` and the user re-picks windows each
//! session.

use crate::key::Key;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Index into [`Config::slots`]. A `u8` is plenty for the 2..=10 supported slots.
pub type SlotIndex = u8;

/// The smallest number of slots the controller allows.
pub const MIN_SLOTS: usize = 2;
/// The largest number of slots the controller allows.
pub const MAX_SLOTS: usize = 10;

/// The complete, serializable controller configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The ordered list of toon slots; always within `MIN_SLOTS..=MAX_SLOTS`.
    pub slots: Vec<Slot>,
    /// Key bindings consulted in multi-toon mode. Each `physical_key` is unique.
    pub bindings: Vec<Binding>,
    /// Key that toggles between multi-toon and individual control modes.
    pub toggle_individual_mode_hotkey: Key,
    /// Key that, while in individual mode, advances the active slot.
    pub cycle_individual_slot_hotkey: Key,
}

impl Default for Config {
    /// A minimal but working starting point: the smallest supported toon count with
    /// default movement bindings. Users scale to their own count with the one-click
    /// "Quick setup" ([`Config::for_toon_count`]).
    fn default() -> Self {
        Config::for_toon_count(MIN_SLOTS)
    }
}

impl Config {
    /// Loads the config from `path`, or returns [`Config::default`] if the file does
    /// not yet exist. A malformed file is reported as an error rather than silently
    /// discarded, so the user can fix or delete it deliberately.
    pub fn load(path: &PathBuf) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading config from {}", path.display()))?;
        let config = serde_json::from_str(&text)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        Ok(config)
    }

    /// Writes the config to `path` as pretty JSON, creating parent directories as
    /// needed.
    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating config directory {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self).context("serializing config")?;
        fs::write(path, text).with_context(|| format!("writing config to {}", path.display()))?;
        Ok(())
    }

    /// Builds a complete setup for `toon_count` toons: that many generically-labeled
    /// slots plus default movement bindings broadcast to all of them. The count is
    /// clamped to the supported range. Powers the UI's one-click "Quick setup", so a
    /// player with any number of toons gets a ready-to-use config without hand-editing.
    pub fn for_toon_count(toon_count: usize) -> Config {
        let count = toon_count.clamp(MIN_SLOTS, MAX_SLOTS);
        let slots = (1..=count)
            .map(|number| Slot::new(&format!("Toon {number}")))
            .collect();
        Config {
            slots,
            bindings: Config::default_bindings(count),
            toggle_individual_mode_hotkey: Key::PageUp,
            cycle_individual_slot_hotkey: Key::PageDown,
        }
    }

    /// Movement bindings that broadcast each common movement key to every slot, so all
    /// linked toons walk together: WASD for movement plus Space to jump — the typical
    /// multitooning control set. Also the target of the UI's "Reset to default
    /// bindings" button.
    pub fn default_bindings(slot_count: usize) -> Vec<Binding> {
        const MOVEMENT_KEYS: [Key; 5] = [Key::W, Key::A, Key::S, Key::D, Key::Space];
        MOVEMENT_KEYS
            .iter()
            .map(|&key| Binding {
                physical_key: key,
                outputs: (0..slot_count)
                    .map(|index| KeyOutput {
                        slot_index: index as SlotIndex,
                        output_key: key,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Reports whether the same physical key is bound more than once, which is
    /// invalid and must block a save. Returns the offending key if found.
    pub fn duplicate_binding_key(&self) -> Option<Key> {
        let mut seen: Vec<Key> = Vec::new();
        for binding in &self.bindings {
            if seen.contains(&binding.physical_key) {
                return Some(binding.physical_key);
            }
            seen.push(binding.physical_key);
        }
        None
    }
}

/// One controllable toon: a user-facing label plus the (non-persisted) window it
/// currently targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    /// User-editable display name, e.g. `"Toon 1"`.
    pub label: String,
    /// The native window this slot drives, chosen fresh each session.
    #[serde(skip)]
    pub target: Option<WindowTarget>,
}

impl Slot {
    /// Creates a slot with the given label and no assigned window.
    pub fn new(label: &str) -> Slot {
        Slot {
            label: label.to_string(),
            target: None,
        }
    }
}

/// A mapping from one physical key to one or more per-slot outputs, used only in
/// multi-toon mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    /// The key the user physically presses on the controller window.
    pub physical_key: Key,
    /// Where the press is delivered; one entry per receiving slot.
    pub outputs: Vec<KeyOutput>,
}

/// A single destination for a bound key: which slot, and what key it receives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyOutput {
    /// The receiving slot's index into [`Config::slots`].
    pub slot_index: SlotIndex,
    /// The key delivered to that slot (may differ from the physical key).
    pub output_key: Key,
}

/// A native window handle, tagged per platform. Never persisted, since handles are
/// not stable across launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowTarget {
    /// Windows top-level window handle.
    Windows { hwnd: isize },
    /// macOS owning process id plus Core Graphics window id.
    MacOs { pid: i32, window_id: u32 },
    /// X11 window id.
    Linux { window_id: u32 },
}

/// The on-disk location of the config file, under the platform's standard config dir.
pub fn config_path() -> Result<PathBuf> {
    let project_dirs = directories::ProjectDirs::from("com", "multitoon", "multitoon")
        .context("could not determine a config directory for this platform")?;
    Ok(project_dirs.config_dir().join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_through_serde_json() {
        let mut config = Config::default();
        let original_slot_count = config.slots.len();
        config.slots.push(Slot::new("Extra"));
        // A window target is set, but serde must skip it on the round trip.
        config.slots[0].target = Some(WindowTarget::Linux { window_id: 42 });
        // Replace the default movement bindings with a single known one to assert on.
        config.bindings.clear();
        config.bindings.push(Binding {
            physical_key: Key::W,
            outputs: vec![
                KeyOutput {
                    slot_index: 0,
                    output_key: Key::W,
                },
                KeyOutput {
                    slot_index: 1,
                    output_key: Key::ArrowUp,
                },
            ],
        });

        let json = serde_json::to_string(&config).expect("serialize");
        let restored: Config = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.slots.len(), original_slot_count + 1);
        assert_eq!(restored.slots[0].label, "Toon 1");
        // Targets are intentionally dropped on save and come back as None.
        assert!(restored.slots[0].target.is_none());
        assert_eq!(restored.bindings.len(), 1);
        assert_eq!(restored.bindings[0].physical_key, Key::W);
        assert_eq!(restored.bindings[0].outputs.len(), 2);
        assert_eq!(restored.bindings[0].outputs[1].output_key, Key::ArrowUp);
        assert_eq!(restored.toggle_individual_mode_hotkey, Key::PageUp);
    }
}

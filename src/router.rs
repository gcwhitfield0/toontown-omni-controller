//! Pure key-routing logic: given the current mode and the binding table, decide
//! which slots receive which key for one physical keystroke.
//!
//! This module performs no platform calls and holds no state, which keeps it fully
//! unit-testable and makes it the conceptual heart of the app — see the README.

use crate::config::{Binding, SlotIndex};
use crate::key::Key;

/// How captured keystrokes are routed to slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    /// Default mode: each key is looked up in the binding table and fanned out to
    /// every output slot the binding names.
    MultiToon,
    /// Passthrough mode: every key goes verbatim to a single chosen slot, bypassing
    /// the binding table entirely.
    Individual {
        /// The slot currently receiving all keystrokes.
        active_slot: SlotIndex,
    },
}

/// One resolved delivery: send `output_key` to the slot at `slot_index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dispatch {
    /// The destination slot's index.
    pub slot_index: SlotIndex,
    /// The key to deliver to that slot.
    pub output_key: Key,
}

/// Resolves one physical keystroke into the list of per-slot dispatches it produces.
///
/// In [`RoutingMode::MultiToon`] the key is matched against `bindings` and expands to
/// one dispatch per output, preserving binding order; an unbound key yields an empty
/// list. In [`RoutingMode::Individual`] the binding table is ignored and the key is
/// forwarded as-is to the single active slot.
pub fn route(mode: &RoutingMode, bindings: &[Binding], physical_key: Key) -> Vec<Dispatch> {
    match mode {
        RoutingMode::MultiToon => bindings
            .iter()
            .find(|binding| binding.physical_key == physical_key)
            .map(|binding| {
                binding
                    .outputs
                    .iter()
                    .map(|output| Dispatch {
                        slot_index: output.slot_index,
                        output_key: output.output_key,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        RoutingMode::Individual { active_slot } => vec![Dispatch {
            slot_index: *active_slot,
            output_key: physical_key,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeyOutput;

    /// Builds a `W` binding that walks slots 0, 1, and 2 forward together.
    fn multi_output_binding() -> Vec<Binding> {
        vec![Binding {
            physical_key: Key::W,
            outputs: vec![
                KeyOutput {
                    slot_index: 0,
                    output_key: Key::W,
                },
                KeyOutput {
                    slot_index: 1,
                    output_key: Key::W,
                },
                KeyOutput {
                    slot_index: 2,
                    output_key: Key::ArrowUp,
                },
            ],
        }]
    }

    #[test]
    fn multitoon_expands_multi_output_binding_in_order() {
        let dispatches = route(&RoutingMode::MultiToon, &multi_output_binding(), Key::W);
        assert_eq!(
            dispatches,
            vec![
                Dispatch {
                    slot_index: 0,
                    output_key: Key::W
                },
                Dispatch {
                    slot_index: 1,
                    output_key: Key::W
                },
                Dispatch {
                    slot_index: 2,
                    output_key: Key::ArrowUp
                },
            ]
        );
    }

    #[test]
    fn multitoon_unbound_key_returns_empty() {
        let dispatches = route(&RoutingMode::MultiToon, &multi_output_binding(), Key::Q);
        assert!(dispatches.is_empty());
    }

    #[test]
    fn individual_ignores_bindings_and_targets_active_slot() {
        let mode = RoutingMode::Individual { active_slot: 3 };
        // `W` is bound in multi-toon mode, but individual mode must bypass that.
        let dispatches = route(&mode, &multi_output_binding(), Key::W);
        assert_eq!(
            dispatches,
            vec![Dispatch {
                slot_index: 3,
                output_key: Key::W
            }]
        );

        // An unbound key still produces exactly one passthrough dispatch.
        let unbound = route(&mode, &multi_output_binding(), Key::Q);
        assert_eq!(
            unbound,
            vec![Dispatch {
                slot_index: 3,
                output_key: Key::Q
            }]
        );
    }
}

//! Platform-neutral key identifiers and the conversions that turn them into the
//! per-platform codes needed for window enumeration and key injection.
//!
//! The [`Key`] enum is the single currency the rest of the app speaks. egui hands
//! us `egui::Key` events; we translate them inward with [`Key::from_egui`]. When we
//! inject a keystroke we translate outward with the cfg-gated `to_*` helpers, each
//! of which only exists on the platform that needs it.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A keyboard key the controller understands, independent of any OS encoding.
///
/// The variant set is intentionally limited to the keys Toontown actually uses
/// (movement, gag selection, navigation) so that every variant has a known code
/// on all three target platforms.
#[rustfmt::skip]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4,
    Num5, Num6, Num7, Num8, Num9,
    F1, F2, F3, F4, F5, F6,
    F7, F8, F9, F10, F11, F12,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Home, End, PageUp, PageDown, Insert, Delete,
    Space, Enter, Escape, Tab, Backspace,
}

impl Key {
    /// Every key the controller can represent, in display order, for building UI
    /// dropdowns. Kept in sync with the enum by hand because Rust has no built-in
    /// variant enumeration.
    #[rustfmt::skip]
    pub fn all() -> &'static [Key] {
        use Key::*;
        &[
            A, B, C, D, E, F, G, H, I, J, K, L, M,
            N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
            Num0, Num1, Num2, Num3, Num4,
            Num5, Num6, Num7, Num8, Num9,
            F1, F2, F3, F4, F5, F6,
            F7, F8, F9, F10, F11, F12,
            ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
            Home, End, PageUp, PageDown, Insert, Delete,
            Space, Enter, Escape, Tab, Backspace,
        ]
    }

    /// Translates an `egui::Key` from a focused-window input event into our neutral
    /// [`Key`]. Returns `None` for keys outside our supported set so the caller can
    /// silently ignore them.
    #[rustfmt::skip]
    pub fn from_egui(key: egui::Key) -> Option<Key> {
        use egui::Key as E;
        let mapped = match key {
            E::A => Key::A, E::B => Key::B, E::C => Key::C, E::D => Key::D,
            E::E => Key::E, E::F => Key::F, E::G => Key::G, E::H => Key::H,
            E::I => Key::I, E::J => Key::J, E::K => Key::K, E::L => Key::L,
            E::M => Key::M, E::N => Key::N, E::O => Key::O, E::P => Key::P,
            E::Q => Key::Q, E::R => Key::R, E::S => Key::S, E::T => Key::T,
            E::U => Key::U, E::V => Key::V, E::W => Key::W, E::X => Key::X,
            E::Y => Key::Y, E::Z => Key::Z,
            E::Num0 => Key::Num0, E::Num1 => Key::Num1, E::Num2 => Key::Num2,
            E::Num3 => Key::Num3, E::Num4 => Key::Num4, E::Num5 => Key::Num5,
            E::Num6 => Key::Num6, E::Num7 => Key::Num7, E::Num8 => Key::Num8,
            E::Num9 => Key::Num9,
            E::F1 => Key::F1, E::F2 => Key::F2, E::F3 => Key::F3, E::F4 => Key::F4,
            E::F5 => Key::F5, E::F6 => Key::F6, E::F7 => Key::F7, E::F8 => Key::F8,
            E::F9 => Key::F9, E::F10 => Key::F10, E::F11 => Key::F11, E::F12 => Key::F12,
            E::ArrowUp => Key::ArrowUp, E::ArrowDown => Key::ArrowDown,
            E::ArrowLeft => Key::ArrowLeft, E::ArrowRight => Key::ArrowRight,
            E::Home => Key::Home, E::End => Key::End,
            E::PageUp => Key::PageUp, E::PageDown => Key::PageDown,
            E::Insert => Key::Insert, E::Delete => Key::Delete,
            E::Space => Key::Space, E::Enter => Key::Enter, E::Escape => Key::Escape,
            E::Tab => Key::Tab, E::Backspace => Key::Backspace,
            _ => return None,
        };
        Some(mapped)
    }

    /// The Windows virtual-key code (`VK_*`) used as the `wParam` of an injected
    /// `WM_KEYDOWN` / `WM_KEYUP` message.
    #[cfg(target_os = "windows")]
    #[rustfmt::skip]
    pub fn to_windows_vk(self) -> u16 {
        use Key::*;
        match self {
            // Letters map onto their ASCII-uppercase code, digits onto ASCII digits.
            A => 0x41, B => 0x42, C => 0x43, D => 0x44, E => 0x45, F => 0x46,
            G => 0x47, H => 0x48, I => 0x49, J => 0x4A, K => 0x4B, L => 0x4C,
            M => 0x4D, N => 0x4E, O => 0x4F, P => 0x50, Q => 0x51, R => 0x52,
            S => 0x53, T => 0x54, U => 0x55, V => 0x56, W => 0x57, X => 0x58,
            Y => 0x59, Z => 0x5A,
            Num0 => 0x30, Num1 => 0x31, Num2 => 0x32, Num3 => 0x33, Num4 => 0x34,
            Num5 => 0x35, Num6 => 0x36, Num7 => 0x37, Num8 => 0x38, Num9 => 0x39,
            F1 => 0x70, F2 => 0x71, F3 => 0x72, F4 => 0x73, F5 => 0x74, F6 => 0x75,
            F7 => 0x76, F8 => 0x77, F9 => 0x78, F10 => 0x79, F11 => 0x7A, F12 => 0x7B,
            ArrowUp => 0x26, ArrowDown => 0x28, ArrowLeft => 0x25, ArrowRight => 0x27,
            Home => 0x24, End => 0x23, PageUp => 0x21, PageDown => 0x22,
            Insert => 0x2D, Delete => 0x2E,
            Space => 0x20, Enter => 0x0D, Escape => 0x1B, Tab => 0x09, Backspace => 0x08,
        }
    }

    /// Whether this key is an "extended" key on Windows (arrows, navigation block).
    /// The extended bit must be set in the `lParam` so games read the right scan code.
    #[cfg(target_os = "windows")]
    pub fn is_windows_extended(self) -> bool {
        use Key::*;
        matches!(
            self,
            ArrowUp
                | ArrowDown
                | ArrowLeft
                | ArrowRight
                | Home
                | End
                | PageUp
                | PageDown
                | Insert
                | Delete
        )
    }

    /// The macOS virtual key code (`CGKeyCode`) for an ANSI keyboard layout, used
    /// when constructing a `CGEvent` keyboard event.
    #[cfg(target_os = "macos")]
    #[rustfmt::skip]
    pub fn to_macos_keycode(self) -> u16 {
        use Key::*;
        match self {
            A => 0x00, B => 0x0B, C => 0x08, D => 0x02, E => 0x0E, F => 0x03,
            G => 0x05, H => 0x04, I => 0x22, J => 0x26, K => 0x28, L => 0x25,
            M => 0x2E, N => 0x2D, O => 0x1F, P => 0x23, Q => 0x0C, R => 0x0F,
            S => 0x01, T => 0x11, U => 0x20, V => 0x09, W => 0x0D, X => 0x07,
            Y => 0x10, Z => 0x06,
            Num0 => 0x1D, Num1 => 0x12, Num2 => 0x13, Num3 => 0x14, Num4 => 0x15,
            Num5 => 0x17, Num6 => 0x16, Num7 => 0x1A, Num8 => 0x1C, Num9 => 0x19,
            F1 => 0x7A, F2 => 0x78, F3 => 0x63, F4 => 0x76, F5 => 0x60, F6 => 0x61,
            F7 => 0x62, F8 => 0x64, F9 => 0x65, F10 => 0x6D, F11 => 0x67, F12 => 0x6F,
            ArrowUp => 0x7E, ArrowDown => 0x7D, ArrowLeft => 0x7B, ArrowRight => 0x7C,
            Home => 0x73, End => 0x77, PageUp => 0x74, PageDown => 0x79,
            Insert => 0x72, Delete => 0x75,
            Space => 0x31, Enter => 0x24, Escape => 0x35, Tab => 0x30, Backspace => 0x33,
        }
    }

    /// The X11 keysym for this key, later resolved to a server-specific keycode via
    /// the keyboard mapping before a synthetic event is sent.
    #[cfg(target_os = "linux")]
    #[rustfmt::skip]
    pub fn to_x11_keysym(self) -> u32 {
        use Key::*;
        match self {
            // Lowercase Latin-1 keysyms for letters, ASCII for digits.
            A => 0x61, B => 0x62, C => 0x63, D => 0x64, E => 0x65, F => 0x66,
            G => 0x67, H => 0x68, I => 0x69, J => 0x6A, K => 0x6B, L => 0x6C,
            M => 0x6D, N => 0x6E, O => 0x6F, P => 0x70, Q => 0x71, R => 0x72,
            S => 0x73, T => 0x74, U => 0x75, V => 0x76, W => 0x77, X => 0x78,
            Y => 0x79, Z => 0x7A,
            Num0 => 0x30, Num1 => 0x31, Num2 => 0x32, Num3 => 0x33, Num4 => 0x34,
            Num5 => 0x35, Num6 => 0x36, Num7 => 0x37, Num8 => 0x38, Num9 => 0x39,
            F1 => 0xFFBE, F2 => 0xFFBF, F3 => 0xFFC0, F4 => 0xFFC1, F5 => 0xFFC2,
            F6 => 0xFFC3, F7 => 0xFFC4, F8 => 0xFFC5, F9 => 0xFFC6, F10 => 0xFFC7,
            F11 => 0xFFC8, F12 => 0xFFC9,
            ArrowUp => 0xFF52, ArrowDown => 0xFF54, ArrowLeft => 0xFF51, ArrowRight => 0xFF53,
            Home => 0xFF50, End => 0xFF57, PageUp => 0xFF55, PageDown => 0xFF56,
            Insert => 0xFF63, Delete => 0xFFFF,
            Space => 0x20, Enter => 0xFF0D, Escape => 0xFF1B, Tab => 0xFF09, Backspace => 0xFF08,
        }
    }
}

impl fmt::Display for Key {
    /// Renders a short human label, stripping the `Num` prefix from digit keys so the
    /// UI shows `0`–`9` rather than `Num0`–`Num9`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = format!("{self:?}");
        let label = text.strip_prefix("Num").unwrap_or(&text);
        formatter.write_str(label)
    }
}

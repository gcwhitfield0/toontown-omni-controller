//! macOS implementation of [`Platform`]. Enumerates windows with
//! `CGWindowListCopyWindowInfo` and injects keystrokes with
//! `CGEventCreateKeyboardEvent` posted to the owning process.
//!
//! The user must grant this binary Accessibility permission (System Settings →
//! Privacy & Security → Accessibility) before key injection is allowed; see the
//! README.

use crate::config::WindowTarget;
use crate::key::Key;
use crate::platform::{Platform, WindowInfo};
use anyhow::{anyhow, Result};
use core::ffi::c_void;
use core_foundation::base::TCFType;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::window::{
    create_window_list, kCGNullWindowID, kCGWindowBounds, kCGWindowListOptionAll,
    kCGWindowListOptionOnScreenOnly, kCGWindowNumber, kCGWindowOwnerName, kCGWindowOwnerPID,
};

/// Case-insensitive substring a window owner/title must contain to count as Toontown.
const TOONTOWN_TITLE_FILTER: &str = "toontown";

/// The macOS [`Platform`] implementation.
pub struct MacOsPlatform;

impl Platform for MacOsPlatform {
    fn list_target_windows(&self) -> Result<Vec<WindowInfo>> {
        enumerate_toontown_windows()
    }

    fn window_at_cursor(&self) -> Result<Option<WindowInfo>> {
        window_under_cursor()
    }

    fn send_key_down(&self, target: &WindowTarget, key: Key) -> Result<()> {
        post_keyboard_event(target, key, true)
    }

    fn send_key_up(&self, target: &WindowTarget, key: Key) -> Result<()> {
        post_keyboard_event(target, key, false)
    }

    fn set_highlight(&self, _highlight: Option<crate::platform::Highlight>) -> Result<()> {
        // Overlay highlighting is currently implemented only on X11.
        Ok(())
    }
}

/// Copies the on-screen window list and keeps entries owned by a Toontown process.
fn enumerate_toontown_windows() -> Result<Vec<WindowInfo>> {
    let window_list = create_window_list(kCGWindowListOptionAll, kCGNullWindowID)
        .ok_or_else(|| anyhow!("CGWindowListCopyWindowInfo returned null"))?;

    let mut windows = Vec::new();
    for item in window_list.iter() {
        // Each array element is a CFDictionary describing one window.
        let dictionary = unsafe { CFDictionary::wrap_under_get_rule(*item as *const _) };
        if let Some(info) = window_info_from(&dictionary) {
            if info.title.to_lowercase().contains(TOONTOWN_TITLE_FILTER) {
                windows.push(info);
            }
        }
    }
    Ok(windows)
}

/// Finds the topmost on-screen Toontown window under the OS cursor. The on-screen
/// list is ordered front-to-back, so the first containing match is the topmost.
fn window_under_cursor() -> Result<Option<WindowInfo>> {
    let (pointer_x, pointer_y) = cursor_location()?;
    let window_list = create_window_list(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
        .ok_or_else(|| anyhow!("CGWindowListCopyWindowInfo returned null"))?;
    for item in window_list.iter() {
        let dictionary = unsafe { CFDictionary::wrap_under_get_rule(*item as *const _) };
        let Some(info) = window_info_from(&dictionary) else {
            continue;
        };
        if !info.title.to_lowercase().contains(TOONTOWN_TITLE_FILTER) {
            continue;
        }
        if bounds_contain(&dictionary, pointer_x, pointer_y) {
            return Ok(Some(info));
        }
    }
    Ok(None)
}

/// Reads the current global mouse-cursor location (top-left origin) via a throwaway
/// Core Graphics event, whose location reflects the live pointer position.
fn cursor_location() -> Result<(f64, f64)> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("could not create a CGEventSource"))?;
    let event = CGEvent::new(source).map_err(|_| anyhow!("could not read the cursor location"))?;
    let location = event.location();
    Ok((location.x, location.y))
}

/// Tests whether a window's `kCGWindowBounds` rectangle contains the given point.
fn bounds_contain(dictionary: &CFDictionary, pointer_x: f64, pointer_y: f64) -> bool {
    let Some(bounds) = read_dict(dictionary, unsafe { kCGWindowBounds }) else {
        return false;
    };
    let left = bounds_value(&bounds, "X").unwrap_or(0.0);
    let top = bounds_value(&bounds, "Y").unwrap_or(0.0);
    let width = bounds_value(&bounds, "Width").unwrap_or(0.0);
    let height = bounds_value(&bounds, "Height").unwrap_or(0.0);
    pointer_x >= left && pointer_x < left + width && pointer_y >= top && pointer_y < top + height
}

/// Reads a nested CFDictionary (e.g. the bounds rectangle) out of a window dictionary.
fn read_dict(dictionary: &CFDictionary, key: CFStringRef) -> Option<CFDictionary> {
    let value = dictionary.find(key as *const c_void)?;
    Some(unsafe { CFDictionary::wrap_under_get_rule(*value as CFDictionaryRef) })
}

/// Reads a named `f64` (e.g. `"Width"`) out of a bounds dictionary.
fn bounds_value(bounds: &CFDictionary, key: &str) -> Option<f64> {
    let cf_key = CFString::new(key);
    let value = bounds.find(cf_key.as_concrete_TypeRef() as *const c_void)?;
    let number = unsafe { CFNumber::wrap_under_get_rule(*value as CFNumberRef) };
    number.to_f64()
}

/// Extracts a [`WindowInfo`] from one window-description dictionary, or `None` if a
/// required field is missing.
fn window_info_from(dictionary: &CFDictionary) -> Option<WindowInfo> {
    let owner = read_string(dictionary, unsafe { kCGWindowOwnerName })?;
    let pid = read_i64(dictionary, unsafe { kCGWindowOwnerPID })? as i32;
    let window_id = read_i64(dictionary, unsafe { kCGWindowNumber })? as u32;
    Some(WindowInfo {
        title: owner,
        target: WindowTarget::MacOs { pid, window_id },
    })
}

/// Reads a CFString value out of the window dictionary as an owned `String`.
fn read_string(dictionary: &CFDictionary, key: CFStringRef) -> Option<String> {
    let value = dictionary.find(key as *const c_void)?;
    let string = unsafe { CFString::wrap_under_get_rule(*value as CFStringRef) };
    Some(string.to_string())
}

/// Reads a CFNumber value out of the window dictionary as an `i64`.
fn read_i64(dictionary: &CFDictionary, key: CFStringRef) -> Option<i64> {
    let value = dictionary.find(key as *const c_void)?;
    let number = unsafe { CFNumber::wrap_under_get_rule(*value as CFNumberRef) };
    number.to_i64()
}

/// Builds a keyboard event for `key` and posts it to the target's owning process.
fn post_keyboard_event(target: &WindowTarget, key: Key, pressed: bool) -> Result<()> {
    let pid = match target {
        WindowTarget::MacOs { pid, .. } => *pid,
        _ => {
            return Err(anyhow!(
                "non-macOS window target passed to the macOS platform"
            ))
        }
    };
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("could not create a CGEventSource"))?;
    let event = CGEvent::new_keyboard_event(source, key.to_macos_keycode(), pressed)
        .map_err(|_| anyhow!("could not create a keyboard event"))?;
    event.post_to_pid(pid);
    Ok(())
}

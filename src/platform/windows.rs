//! Windows implementation of [`Platform`]. Enumerates top-level windows with
//! `EnumWindows` and injects keystrokes by posting `WM_KEYDOWN` / `WM_KEYUP` to a
//! window's message queue, so the target need not be focused.

use crate::config::WindowTarget;
use crate::key::Key;
use crate::platform::{Platform, WindowInfo};
use anyhow::{anyhow, Result};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetCursorPos, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    PostMessageW, WindowFromPoint, GA_ROOT, WM_KEYDOWN, WM_KEYUP,
};

/// Case-insensitive substring a window title must contain to be treated as Toontown.
const TOONTOWN_TITLE_FILTER: &str = "toontown";

/// The Windows [`Platform`] implementation.
pub struct WindowsPlatform;

impl Platform for WindowsPlatform {
    fn list_target_windows(&self) -> Result<Vec<WindowInfo>> {
        enumerate_toontown_windows()
    }

    fn window_at_cursor(&self) -> Result<Option<WindowInfo>> {
        window_under_cursor()
    }

    fn send_key_down(&self, target: &WindowTarget, key: Key) -> Result<()> {
        post_key_message(target, key, true)
    }

    fn send_key_up(&self, target: &WindowTarget, key: Key) -> Result<()> {
        post_key_message(target, key, false)
    }

    fn set_highlight(&self, _highlight: Option<crate::platform::Highlight>) -> Result<()> {
        // Overlay highlighting is currently implemented only on X11.
        Ok(())
    }
}

/// Drives `EnumWindows`, collecting visible Toontown windows into a vector.
fn enumerate_toontown_windows() -> Result<Vec<WindowInfo>> {
    let mut collected: Vec<WindowInfo> = Vec::new();
    let user_data = LPARAM(&mut collected as *mut Vec<WindowInfo> as isize);
    // SAFETY: `collect_window` only runs for the duration of this call, and
    // `user_data` points at `collected`, which outlives the enumeration.
    unsafe { EnumWindows(Some(collect_window), user_data)? };
    Ok(collected)
}

/// `EnumWindows` callback: appends matching windows to the vector behind `lparam`.
unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    if let Some(title) = window_title(hwnd) {
        if title.to_lowercase().contains(TOONTOWN_TITLE_FILTER) {
            let collected = &mut *(lparam.0 as *mut Vec<WindowInfo>);
            collected.push(WindowInfo {
                title,
                target: WindowTarget::Windows {
                    hwnd: hwnd.0 as isize,
                },
            });
        }
    }
    BOOL(1) // keep enumerating
}

/// Reads a window's title text, returning `None` for untitled windows.
unsafe fn window_title(hwnd: HWND) -> Option<String> {
    let length = GetWindowTextLengthW(hwnd);
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let copied = GetWindowTextW(hwnd, &mut buffer);
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

/// Finds the top-level window under the OS cursor and returns it if it is a Toontown
/// window. `WindowFromPoint` yields the deepest child, so we walk up to its root.
fn window_under_cursor() -> Result<Option<WindowInfo>> {
    let mut point = POINT::default();
    // SAFETY: GetCursorPos writes into our owned POINT and has no other preconditions.
    unsafe { GetCursorPos(&mut point)? };
    // SAFETY: WindowFromPoint/GetAncestor take a value and a handle; both are total.
    let top_level = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
    if top_level.0.is_null() {
        return Ok(None);
    }
    // SAFETY: `top_level` is a live top-level handle from GetAncestor.
    let Some(title) = (unsafe { window_title(top_level) }) else {
        return Ok(None);
    };
    if !title.to_lowercase().contains(TOONTOWN_TITLE_FILTER) {
        return Ok(None);
    }
    Ok(Some(WindowInfo {
        title,
        target: WindowTarget::Windows {
            hwnd: top_level.0 as isize,
        },
    }))
}

/// Posts a single key message to the target window's message queue.
fn post_key_message(target: &WindowTarget, key: Key, pressed: bool) -> Result<()> {
    let hwnd = match target {
        WindowTarget::Windows { hwnd } => HWND(*hwnd as *mut core::ffi::c_void),
        _ => {
            return Err(anyhow!(
                "non-Windows window target passed to the Windows platform"
            ))
        }
    };
    let virtual_key = key.to_windows_vk();
    let message = if pressed { WM_KEYDOWN } else { WM_KEYUP };
    let lparam = key_message_lparam(key, virtual_key, pressed);
    // SAFETY: `hwnd` came from enumeration; PostMessageW is non-blocking and safe to
    // call with a stale handle (it simply fails, which we surface as an error).
    unsafe { PostMessageW(hwnd, message, WPARAM(virtual_key as usize), lparam)? };
    Ok(())
}

/// Builds the `lParam` bitfield that accompanies a `WM_KEYDOWN`/`WM_KEYUP` message,
/// encoding repeat count, scan code, the extended-key flag, and the transition bits.
fn key_message_lparam(key: Key, virtual_key: u16, pressed: bool) -> LPARAM {
    // SAFETY: MapVirtualKeyW has no preconditions and cannot fail destructively.
    let scan_code = unsafe { MapVirtualKeyW(virtual_key as u32, MAPVK_VK_TO_VSC) } & 0xFF;
    let mut value: u32 = 1 | (scan_code << 16); // repeat count = 1
    if key.is_windows_extended() {
        value |= 1 << 24; // extended-key flag
    }
    if !pressed {
        value |= (1 << 30) | (1 << 31); // previous-state + transition (key up)
    }
    LPARAM(value as isize)
}

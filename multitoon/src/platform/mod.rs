//! The OS abstraction: enumerate candidate game windows and inject key events into
//! them without those windows holding focus.
//!
//! Each supported OS implements [`Platform`] in its own cfg-gated file. The rest of
//! the app only ever sees the trait and the neutral [`WindowInfo`] / `WindowTarget`
//! types, so UI and routing code stays platform-free.

use crate::config::WindowTarget;
use crate::key::Key;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// A window discovered during enumeration: what to show the user, and the handle to
/// drive it.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    /// The window's title bar text, shown in the picker dropdown.
    pub title: String,
    /// The opaque native handle used for injection.
    pub target: WindowTarget,
}

/// A request to draw a highlight (outline + label bubble) over a target window. The
/// UI rebuilds this every frame; `None` to [`Platform::set_highlight`] clears it.
#[derive(Debug, Clone)]
pub struct Highlight {
    /// The window to frame.
    pub target: WindowTarget,
    /// Short text shown in the label bubble (typically the slot's label).
    pub label: String,
}

/// The behaviors the controller needs from the host OS.
///
/// `Send` is required so the implementation can be stored behind a `Box<dyn Platform>`
/// inside the app state without thread-affinity surprises.
pub trait Platform: Send {
    /// Enumerates top-level windows that look like Toontown instances.
    fn list_target_windows(&self) -> anyhow::Result<Vec<WindowInfo>>;
    /// Returns the topmost Toontown window currently under the OS mouse cursor, if
    /// any. This backs the crosshair "drag onto a game window" picker: the UI calls
    /// it the instant the drag is released to learn what window the cursor landed on.
    fn window_at_cursor(&self) -> anyhow::Result<Option<WindowInfo>>;
    /// Injects a key-press into `target`.
    fn send_key_down(&self, target: &WindowTarget, key: Key) -> anyhow::Result<()>;
    /// Injects a key-release into `target`.
    fn send_key_up(&self, target: &WindowTarget, key: Key) -> anyhow::Result<()>;
    /// Shows an outline + label over a window, or clears any current highlight when
    /// passed `None`. Implementations that don't support overlays may no-op.
    fn set_highlight(&self, highlight: Option<Highlight>) -> anyhow::Result<()>;
}

/// Returns the [`Platform`] implementation for the OS this binary was built for.
#[cfg(target_os = "windows")]
pub fn current_platform() -> Box<dyn Platform> {
    Box::new(windows::WindowsPlatform)
}

/// Returns the [`Platform`] implementation for the OS this binary was built for.
#[cfg(target_os = "macos")]
pub fn current_platform() -> Box<dyn Platform> {
    Box::new(macos::MacOsPlatform)
}

/// Returns the [`Platform`] implementation for the OS this binary was built for.
#[cfg(target_os = "linux")]
pub fn current_platform() -> Box<dyn Platform> {
    Box::new(linux::LinuxPlatform::new())
}

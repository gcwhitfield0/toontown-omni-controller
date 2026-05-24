//! X11 implementation of [`Platform`]. Enumerates windows from `_NET_CLIENT_LIST`
//! on the root window and injects synthetic `KeyPress`/`KeyRelease` events.
//!
//! Wayland is not supported; under a Wayland session this works only through the
//! XWayland compatibility layer. A fresh connection is opened per call, which keeps
//! the type free of stored state at the cost of a little reconnection overhead.

use crate::config::WindowTarget;
use crate::key::Key;
use crate::platform::{Highlight, Platform, WindowInfo};
use anyhow::{anyhow, Context, Result};
use std::cell::RefCell;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Gcontext,
    KeyButMask, KeyPressEvent, StackMode, Window, WindowClass, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
};
use x11rb::rust_connection::RustConnection;

/// Case-insensitive substring a window title must contain to be treated as Toontown.
const TOONTOWN_TITLE_FILTER: &str = "toontown";

/// The X11 [`Platform`] implementation. Window enumeration and key injection open a
/// fresh connection per call; the highlight overlay keeps its own long-lived
/// connection (in [`LinuxPlatform::overlay`]) because its windows must persist.
pub struct LinuxPlatform {
    /// Lazily created overlay state for window highlighting.
    overlay: RefCell<Option<Overlay>>,
}

impl LinuxPlatform {
    /// Creates a platform handle with no overlay yet (built on first highlight).
    pub fn new() -> Self {
        LinuxPlatform {
            overlay: RefCell::new(None),
        }
    }
}

impl Platform for LinuxPlatform {
    fn list_target_windows(&self) -> Result<Vec<WindowInfo>> {
        let (connection, root) = connect()?;
        find_toontown_windows(&connection, root)
    }

    fn window_at_cursor(&self) -> Result<Option<WindowInfo>> {
        let (connection, root) = connect()?;
        let pointer = connection.query_pointer(root)?.reply()?;
        let point = (pointer.root_x as i32, pointer.root_y as i32);

        // Use stacking order (bottom-to-top) and scan top-first so an overlapping
        // window picks the one actually visible under the cursor, not whichever
        // happens to be first in map order.
        let mut stacked =
            find_toontown_windows_in(&connection, root, b"_NET_CLIENT_LIST_STACKING")?;
        if stacked.is_empty() {
            stacked = find_toontown_windows(&connection, root)?;
        }
        for window in stacked.into_iter().rev() {
            if let WindowTarget::Linux { window_id } = window.target {
                if window_contains_point(&connection, root, window_id, point)? {
                    return Ok(Some(window));
                }
            }
        }
        Ok(None)
    }

    fn send_key_down(&self, target: &WindowTarget, key: Key) -> Result<()> {
        inject_key(target, key, true)
    }

    fn send_key_up(&self, target: &WindowTarget, key: Key) -> Result<()> {
        inject_key(target, key, false)
    }

    fn set_highlight(&self, highlight: Option<Highlight>) -> Result<()> {
        let mut guard = self.overlay.borrow_mut();
        match highlight {
            Some(request) => {
                let WindowTarget::Linux { window_id } = request.target else {
                    return Ok(()); // not an X11 target; nothing to frame
                };
                if guard.is_none() {
                    *guard = Some(Overlay::new()?);
                }
                if let Some(overlay) = guard.as_mut() {
                    overlay.show(window_id, &request.label)?;
                }
            }
            None => {
                if let Some(overlay) = guard.as_mut() {
                    overlay.hide()?;
                }
            }
        }
        Ok(())
    }
}

/// Opens an X11 connection and returns it alongside the default screen's root window.
fn connect() -> Result<(RustConnection, Window)> {
    let (connection, screen_num) =
        x11rb::connect(None).context("connecting to the X11 display (is $DISPLAY set?)")?;
    let root = connection.setup().roots[screen_num].root;
    Ok((connection, root))
}

/// Reads `_NET_CLIENT_LIST` and keeps only windows whose title looks like Toontown.
fn find_toontown_windows(connection: &RustConnection, root: Window) -> Result<Vec<WindowInfo>> {
    find_toontown_windows_in(connection, root, b"_NET_CLIENT_LIST")
}

/// Reads the window-id list from the named root property and keeps only windows whose
/// title looks like Toontown, preserving the property's ordering.
fn find_toontown_windows_in(
    connection: &RustConnection,
    root: Window,
    property_name: &[u8],
) -> Result<Vec<WindowInfo>> {
    let list_atom = connection.intern_atom(false, property_name)?.reply()?.atom;
    let reply = connection
        .get_property(false, root, list_atom, AtomEnum::WINDOW, 0, u32::MAX)?
        .reply()?;
    // A missing property (e.g. no stacking hint) yields an empty list, not an error.
    let Some(window_ids) = reply.value32() else {
        return Ok(Vec::new());
    };

    let mut windows = Vec::new();
    for window in window_ids {
        let title = window_title(connection, window)?;
        if title.to_lowercase().contains(TOONTOWN_TITLE_FILTER) {
            windows.push(WindowInfo {
                title,
                target: WindowTarget::Linux { window_id: window },
            });
        }
    }
    Ok(windows)
}

/// Fetches a window's title, preferring the UTF-8 `_NET_WM_NAME` over legacy `WM_NAME`.
fn window_title(connection: &RustConnection, window: Window) -> Result<String> {
    let net_wm_name = connection
        .intern_atom(false, b"_NET_WM_NAME")?
        .reply()?
        .atom;
    let utf8_string = connection.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
    let reply = connection
        .get_property(false, window, net_wm_name, utf8_string, 0, u32::MAX)?
        .reply()?;
    if !reply.value.is_empty() {
        return Ok(String::from_utf8_lossy(&reply.value).into_owned());
    }

    // Fall back to the older Latin-1 WM_NAME property.
    let legacy = connection
        .get_property(
            false,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            0,
            u32::MAX,
        )?
        .reply()?;
    Ok(String::from_utf8_lossy(&legacy.value).into_owned())
}

/// Reports whether `window`'s on-screen rectangle contains the root-relative point.
fn window_contains_point(
    connection: &RustConnection,
    root: Window,
    window: Window,
    point: (i32, i32),
) -> Result<bool> {
    let (x, y, width, height) = absolute_rect(connection, root, window)?;
    let left = x as i32;
    let top = y as i32;
    let right = left + width as i32;
    let bottom = top + height as i32;
    let (pointer_x, pointer_y) = point;
    Ok(pointer_x >= left && pointer_x < right && pointer_y >= top && pointer_y < bottom)
}

/// Computes a window's absolute on-screen rectangle (origin in root coordinates).
///
/// Under reparenting window managers a window's own geometry is relative to its frame,
/// so we translate its origin into root coordinates.
fn absolute_rect(
    connection: &RustConnection,
    root: Window,
    window: Window,
) -> Result<(i16, i16, u16, u16)> {
    let geometry = connection.get_geometry(window)?.reply()?;
    let origin = connection
        .translate_coordinates(window, root, 0, 0)?
        .reply()?;
    Ok((origin.dst_x, origin.dst_y, geometry.width, geometry.height))
}

/// Resolves a key to a server keycode and sends one synthetic event to its window.
fn inject_key(target: &WindowTarget, key: Key, pressed: bool) -> Result<()> {
    let window = match target {
        WindowTarget::Linux { window_id } => *window_id,
        _ => {
            return Err(anyhow!(
                "non-X11 window target passed to the Linux platform"
            ))
        }
    };
    let (connection, root) = connect()?;
    let keycode = keysym_to_keycode(&connection, key.to_x11_keysym())?
        .ok_or_else(|| anyhow!("no keycode maps to keysym for {key}"))?;
    send_synthetic_key(&connection, root, window, keycode, pressed)
}

/// Searches the server's keyboard mapping for the keycode that produces `keysym`.
fn keysym_to_keycode(connection: &RustConnection, keysym: u32) -> Result<Option<u8>> {
    let setup = connection.setup();
    let min_keycode = setup.min_keycode;
    let count = setup.max_keycode - min_keycode + 1;
    let mapping = connection
        .get_keyboard_mapping(min_keycode, count)?
        .reply()?;
    let per_code = mapping.keysyms_per_keycode as usize;

    for (index, candidate) in mapping.keysyms.iter().enumerate() {
        if *candidate == keysym {
            let keycode = min_keycode as usize + index / per_code;
            return Ok(Some(keycode as u8));
        }
    }
    Ok(None)
}

/// Sends a single synthetic `KeyPress` or `KeyRelease` directly to `window`.
fn send_synthetic_key(
    connection: &RustConnection,
    root: Window,
    window: Window,
    keycode: u8,
    pressed: bool,
) -> Result<()> {
    let response_type = if pressed {
        KEY_PRESS_EVENT
    } else {
        KEY_RELEASE_EVENT
    };
    let event_mask = if pressed {
        EventMask::KEY_PRESS
    } else {
        EventMask::KEY_RELEASE
    };
    let event = KeyPressEvent {
        response_type,
        detail: keycode,
        sequence: 0,
        time: x11rb::CURRENT_TIME,
        root,
        event: window,
        child: x11rb::NONE,
        root_x: 0,
        root_y: 0,
        event_x: 0,
        event_y: 0,
        state: KeyButMask::default(),
        same_screen: true,
    };
    connection.send_event(false, window, event_mask, event)?;
    connection.flush()?;
    Ok(())
}

// --- Window highlight overlay ------------------------------------------------

/// Border thickness, in pixels, of the highlight frame.
const BORDER_THICKNESS: u16 = 3;
/// Border colour (assumes a TrueColor visual): bright green, like the reference tool.
const BORDER_PIXEL: u32 = 0x0000_FF00;
/// Background colour of the label bubble.
const LABEL_BG_PIXEL: u32 = 0x0020_2020;
/// Text colour of the label bubble.
const LABEL_FG_PIXEL: u32 = 0x00FF_FFFF;
/// Height of the label bubble, in pixels.
const LABEL_HEIGHT: u16 = 16;
/// Approximate per-character width of the built-in `fixed` font, for sizing the label.
const LABEL_CHAR_WIDTH: u16 = 6;
/// Largest label length we draw, to stay well under `image_text8`'s 255-byte limit.
const LABEL_MAX_CHARS: usize = 40;

/// Long-lived override-redirect windows that draw a frame and label over a target
/// window. Four thin bars form the border (leaving the interior click-through) and one
/// small window holds the label text.
struct Overlay {
    connection: RustConnection,
    root: Window,
    borders: [Window; 4],
    label_window: Window,
    label_gc: Gcontext,
    /// The (window, label) currently displayed, to skip redundant redraws.
    shown: Option<(Window, String)>,
}

impl Overlay {
    /// Opens a dedicated connection and creates the (initially unmapped) overlay
    /// windows once; subsequent highlights just reposition and map them.
    fn new() -> Result<Overlay> {
        let (connection, screen_num) =
            x11rb::connect(None).context("opening X11 connection for the highlight overlay")?;
        let (root, depth, visual) = {
            let screen = &connection.setup().roots[screen_num];
            (screen.root, screen.root_depth, screen.root_visual)
        };
        let mut borders = [0u32; 4];
        for border in &mut borders {
            *border = create_overlay_window(&connection, root, depth, visual, BORDER_PIXEL)?;
        }
        let label_window = create_overlay_window(&connection, root, depth, visual, LABEL_BG_PIXEL)?;
        let label_gc = create_label_gc(&connection, label_window)?;
        Ok(Overlay {
            connection,
            root,
            borders,
            label_window,
            label_gc,
            shown: None,
        })
    }

    /// Frames `window` and shows `label`, repositioning the bars to its current bounds.
    fn show(&mut self, window: Window, label: &str) -> Result<()> {
        if self.shown.as_ref() == Some(&(window, label.to_string())) {
            return Ok(()); // already displaying exactly this
        }
        let (x, y, width, height) = absolute_rect(&self.connection, self.root, window)?;
        self.place_borders(x, y, width, height)?;
        self.place_label(x, y, label)?;
        self.connection.flush()?;
        self.shown = Some((window, label.to_string()));
        Ok(())
    }

    /// Hides all overlay windows, if anything is currently shown.
    fn hide(&mut self) -> Result<()> {
        if self.shown.is_none() {
            return Ok(());
        }
        for &border in &self.borders {
            self.connection.unmap_window(border)?;
        }
        self.connection.unmap_window(self.label_window)?;
        self.connection.flush()?;
        self.shown = None;
        Ok(())
    }

    /// Positions and maps the four border bars around the given rectangle.
    fn place_borders(&self, x: i16, y: i16, width: u16, height: u16) -> Result<()> {
        let thickness = BORDER_THICKNESS;
        let right = x + width.saturating_sub(thickness) as i16;
        let bottom = y + height.saturating_sub(thickness) as i16;
        let bars = [
            (x, y, width, thickness),      // top
            (x, bottom, width, thickness), // bottom
            (x, y, thickness, height),     // left
            (right, y, thickness, height), // right
        ];
        for (window, bar) in self.borders.iter().zip(bars) {
            configure_overlay_window(&self.connection, *window, bar)?;
            self.connection.map_window(*window)?;
        }
        Ok(())
    }

    /// Positions, maps, and draws the label bubble near the top-left of the rectangle.
    fn place_label(&self, x: i16, y: i16, label: &str) -> Result<()> {
        let text = clamp_label(label);
        let width = text.len() as u16 * LABEL_CHAR_WIDTH + 8;
        let offset = BORDER_THICKNESS as i16 + 2;
        let bar = (x + offset, y + offset, width, LABEL_HEIGHT);
        configure_overlay_window(&self.connection, self.label_window, bar)?;
        self.connection.map_window(self.label_window)?;
        // Clear any stale text, then draw the new label at a baseline inside the bubble.
        self.connection
            .clear_area(false, self.label_window, 0, 0, 0, 0)?;
        self.connection
            .image_text8(self.label_window, self.label_gc, 4, 12, text.as_bytes())?;
        Ok(())
    }
}

/// Creates one solid-colour, override-redirect overlay window (initially 1×1, hidden).
fn create_overlay_window(
    connection: &RustConnection,
    root: Window,
    depth: u8,
    visual: u32,
    pixel: u32,
) -> Result<Window> {
    let window = connection.generate_id()?;
    let aux = CreateWindowAux::new()
        .override_redirect(1)
        .background_pixel(pixel)
        .event_mask(EventMask::NO_EVENT);
    connection.create_window(
        depth,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        visual,
        &aux,
    )?;
    Ok(window)
}

/// Builds a graphics context for label text using the server's built-in `fixed` font.
fn create_label_gc(connection: &RustConnection, window: Window) -> Result<Gcontext> {
    let font = connection.generate_id()?;
    connection.open_font(font, b"fixed")?;
    let gc = connection.generate_id()?;
    let aux = CreateGCAux::new()
        .foreground(LABEL_FG_PIXEL)
        .background(LABEL_BG_PIXEL)
        .font(font);
    connection.create_gc(gc, window, &aux)?;
    connection.close_font(font)?; // the GC keeps its own reference to the font
    Ok(gc)
}

/// Moves/resizes an overlay window and raises it above other windows.
fn configure_overlay_window(
    connection: &RustConnection,
    window: Window,
    rect: (i16, i16, u16, u16),
) -> Result<()> {
    let (x, y, width, height) = rect;
    let aux = ConfigureWindowAux::new()
        .x(x as i32)
        .y(y as i32)
        .width(width.max(1) as u32)
        .height(height.max(1) as u32)
        .stack_mode(StackMode::ABOVE);
    connection.configure_window(window, &aux)?;
    Ok(())
}

/// Trims a label to a sane on-screen length.
fn clamp_label(label: &str) -> String {
    if label.chars().count() <= LABEL_MAX_CHARS {
        return label.to_string();
    }
    label.chars().take(LABEL_MAX_CHARS).collect()
}

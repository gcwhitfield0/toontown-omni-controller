# multitoon

A minimal, cross-platform Toontown multicontroller. It captures keystrokes while its
own window is focused and replays them to multiple Toontown game windows at once, so a
single keyboard can drive 2–10 toons together for cooperative play and grinding.

## Build and run

```sh
cargo run --release
```

This produces and launches a single native binary — there is no separate frontend or
runtime to install. The compiled binary is written to `target/release/multitoon`
(`multitoon.exe` on Windows); you can copy it anywhere and run it directly.

Prerequisites: a stable Rust toolchain (install via [rustup](https://rustup.rs)), plus
the per-platform requirements in **Platform setup** below.

## Quick start

A first run, end to end:

1. **Launch Toontown** two or more times so you have several game windows open, and log
   a toon into each.
2. **Start multitoon** with `cargo run --release` (or run the built binary). At the top
   of the left panel, use **Quick setup**: set *Number of toons* to how many you run
   and click **Generate**. That creates that many slots, each pre-bound to WASD + Space
   broadcast to every toon — so you only need to link windows. (You can still fine-tune
   individual slots and bindings afterward; see the advanced editor on the right.)
3. **Link each slot to a window** with its crosshair. Each slot row has a draggable
   **✛ crosshair**: press it and drag the cursor over the Toontown window you want
   that slot to control, then release. multitoon detects the window under the cursor
   and links it; the slot row shows the linked window's title and the status bar
   confirms. Use *Add slot* / *Remove slot* to match your toon count (2–10), and
   rename slots inline (e.g. `Tank`, `Healer`).
   *(The optional **Refresh windows** button just pre-lists open Toontown windows so
   titles display immediately; linking does not require it.)*
5. **Add a binding** — click *Add binding* in the central panel. Set its *Physical key*
   to `W`, then add one output per slot (each *sends* `W`). Now pressing `W` walks every
   assigned toon forward together. Repeat for `A`/`S`/`D`, your gag keys, etc.
6. **Save** with *Save config* in the status bar so your labels and bindings persist.
   (Window assignments are deliberately *not* saved — see below.)
7. **Play.** Keep the multitoon window focused and press your bound keys; they fan out
   to every assigned window. Press **PageUp** to switch to individual mode and
   **PageDown** to cycle which single toon you're talking to; press **PageUp** again to
   return to multi-toon mode.

The rest of this README explains each piece in detail.

## Platform setup

multitoon targets Windows 10/11, macOS 12+, and Linux running X11.

- **Windows** — no extra setup. Windows are enumerated with `EnumWindows`; keystrokes
  are delivered with `PostMessageW` (`WM_KEYDOWN` / `WM_KEYUP`), so target windows do
  not need focus.
- **macOS** — on first run, grant the binary **Accessibility** permission under
  *System Settings → Privacy & Security → Accessibility*. Without it, macOS silently
  blocks the injected keyboard events. Enumeration uses `CGWindowListCopyWindowInfo`
  and injection uses `CGEventCreateKeyboardEvent` posted to the owning process.
- **Linux** — requires an **X11** session. Enumeration reads `_NET_CLIENT_LIST` from
  the root window and injection sends synthetic `KeyPress` / `KeyRelease` events.
  **Wayland-native sessions are not supported**; under Wayland this works only via the
  XWayland compatibility layer, and key injection into native Wayland clients will not
  function.

> **Focus model.** Keystrokes are captured only while the multitoon window itself has
> OS focus. There is no global/background keyboard hook. Keep multitoon focused while
> playing; click it to start sending, click away to stop.

## Usage

### Slots

A *slot* is one controllable toon. The left panel lists your slots:

1. Edit a slot's label (e.g. `Toon 1`) inline.
2. **Link the slot to a window** using its **✛ crosshair**: press the crosshair and,
   keeping the mouse button held, drag the cursor over the target Toontown window,
   then release. The window under the cursor at release becomes the slot's target.
   This mirrors the classic "drag a crosshair onto the window" tools. **Unlink**
   clears the link.
3. **Highlight to confirm which window is which.** Hovering a linked slot's crosshair
   draws a green outline and a label bubble over that slot's window; while you're
   dragging a crosshair, the window currently under the cursor is highlighted as a
   live preview of what you'll link. This is handy because multiple Toontown clients
   often share the same title. *(Overlay highlighting is currently X11-only; on
   Windows/macOS it is a no-op.)*
4. Click **Refresh windows** any time to (re-)list open Toontown windows so their
   titles show next to each slot — optional, since linking enumerates on its own.
5. **Add slot** / **Remove slot** keep the count within the supported range of 2–10.

> **Why drag-to-link and not a saved list?** Native window handles change every
> launch, so links are *not* saved between sessions — re-link each time you start the
> app. Slot labels and key bindings *are* saved.

### Bindings (multi-toon mode)

The central panel is the binding table used in the default **multi-toon** mode. Each
binding maps one physical key to one or more `(slot, output key)` outputs. For example,
binding `W` to slots 0–3 (each outputting `W`) walks all four toons forward at once;
you can also send a *different* output key per slot. A physical key may appear in at
most one binding — duplicates are flagged inline and block saving until fixed.

### Modes

- **Multi-toon (default):** every keystroke is looked up in the binding table and
  fanned out to the slots it names.
- **Individual:** every keystroke is forwarded verbatim to a single chosen slot,
  bypassing the binding table — a "talk to one toon" passthrough.

Two hotkeys (configurable at the bottom of the slots panel, default **PageUp** and
**PageDown**) control modes:

- **Toggle individual mode** flips between multi-toon and individual.
- **Cycle active slot** advances which slot receives keys while in individual mode.

The status bar at the bottom shows the current mode, the active slot in individual
mode, and whether the config has unsaved changes. Click **Save config** to persist.

## Diagnostics

Two headless flags help verify the X11 backend without driving the GUI (neither
injects keys):

- `multitoon --probe` — lists detected Toontown windows, then prints the
  window-under-cursor for ~10 seconds (hover a game window to test the picker).
- `multitoon --probe-highlight` — draws the outline + label overlay over each detected
  window in turn, then clears it.

## Architecture

The conceptual core is two small modules:

- [`src/router.rs`](src/router.rs) — pure, platform-free routing logic. Given the
  current mode and the binding table, `route()` turns one physical keystroke into the
  list of per-slot dispatches it should produce. It has no side effects and is fully
  unit-tested.
- [`src/platform/mod.rs`](src/platform/mod.rs) — the `Platform` trait that abstracts
  window enumeration and key injection, with one cfg-gated implementation per OS
  (`windows.rs`, `macos.rs`, `linux.rs`).

The UI ([`src/ui/`](src/ui)) is immediate-mode egui that owns the config and calls
into these two modules; [`src/config.rs`](src/config.rs) defines the persisted data
model and its JSON load/save; [`src/key.rs`](src/key.rs) defines the platform-neutral
`Key` enum and its per-platform code conversions.

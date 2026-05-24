//! Entry point for the multitoon controller.
//!
//! Loads the persisted config, selects the platform backend, and hands both to the
//! egui application. See [`router`] and [`platform`] for the conceptual core.

mod config;
mod key;
mod platform;
mod router;
mod ui;

use anyhow::{Context, Result};

/// Loads config and launches the native eframe window.
fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--probe") {
        return run_probe();
    }
    if std::env::args().any(|arg| arg == "--probe-highlight") {
        return run_highlight_probe();
    }

    let config_path = config::config_path().context("resolving config path")?;
    let config = config::Config::load(&config_path).context("loading config")?;
    let platform = platform::current_platform();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "multitoon",
        native_options,
        Box::new(move |_creation_context| {
            Ok(Box::new(ui::MultiToonApp::new(
                config,
                config_path,
                platform,
            )))
        }),
    )
    .map_err(|error| anyhow::anyhow!("eframe failed to run: {error}"))?;
    Ok(())
}

/// Headless diagnostic (`--probe`): exercises the real platform backend without the
/// GUI and without injecting any keys. It enumerates Toontown windows, then polls the
/// window-under-cursor for a few seconds so the crosshair picker can be verified by
/// hovering a game window.
fn run_probe() -> Result<()> {
    let platform = platform::current_platform();

    println!("Enumerating Toontown windows...");
    let windows = platform.list_target_windows()?;
    if windows.is_empty() {
        println!("  (none found)");
    }
    for (index, window) in windows.iter().enumerate() {
        println!(
            "  [{index}] title={:?}  target={:?}",
            window.title, window.target
        );
    }

    println!("\nPolling window-under-cursor for ~10s — hover over a Toontown window:");
    for _ in 0..14 {
        match platform.window_at_cursor() {
            Ok(Some(window)) => println!("  cursor over: {:?}", window.title),
            Ok(None) => println!("  cursor over: (no Toontown window)"),
            Err(error) => println!("  error: {error:#}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
    Ok(())
}

/// Headless diagnostic (`--probe-highlight`): draws the outline + label overlay over
/// each enumerated Toontown window in turn for a few seconds, then clears it. Used to
/// verify the X11 overlay renders correctly without driving the GUI.
fn run_highlight_probe() -> Result<()> {
    let platform = platform::current_platform();
    let windows = platform.list_target_windows()?;
    if windows.is_empty() {
        println!("No Toontown windows found to highlight.");
        return Ok(());
    }
    for window in &windows {
        println!("Highlighting {:?} for 3s...", window.title);
        platform.set_highlight(Some(platform::Highlight {
            target: window.target.clone(),
            label: window.title.clone(),
        }))?;
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    platform.set_highlight(None)?;
    println!("Cleared highlight.");
    Ok(())
}

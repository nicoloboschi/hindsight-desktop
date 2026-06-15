// Tray-only app: hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod supervisor;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Wry,
};

const POLL_INTERVAL: Duration = Duration::from_secs(3);

// Status icons baked into the binary.
const ICON_ON: &[u8] = include_bytes!("../icons/tray-on.png"); // green dot = up
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png"); // hollow grey = down
const ICON_STARTING: &[u8] = include_bytes!("../icons/tray-starting.png"); // amber = starting

/// Handles needed to repaint the tray from either the menu handler or the poll
/// loop. Cheap to clone (each wraps an Arc).
#[derive(Clone)]
struct Ui {
    status: MenuItem<Wry>,
    start: MenuItem<Wry>,
}

fn main() {
    // True between a Start/Restart click and the daemon answering /health (or the
    // start attempt giving up). Shared by the menu handler and the poll loop.
    let starting = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .setup({
            let starting = starting.clone();
            move |app| {
                // Keep the Dock icon: the menu-bar icon can be hidden by the
                // notch / menu-bar overflow, so the Dock is the reliable surface.
                // Clicking it (RunEvent::Reopen) opens the control center.
                let none = None::<&str>;
                let ui = Ui {
                    status: MenuItem::with_id(app, "status", "Checking…", false, none)?,
                    start: MenuItem::with_id(app, "start", "Start", true, none)?,
                };
                let control_i =
                    MenuItem::with_id(app, "control", "Open Control Center", true, none)?;
                let quit_i = MenuItem::with_id(app, "quit", "Quit Hindsight Menu", true, none)?;

                let menu = Menu::with_items(
                    app,
                    &[
                        &ui.status,
                        &PredefinedMenuItem::separator(app)?,
                        &ui.start,
                        &control_i,
                        &PredefinedMenuItem::separator(app)?,
                        &quit_i,
                    ],
                )?;

                let menu_ui = ui.clone();
                let menu_starting = starting.clone();
                TrayIconBuilder::with_id("main")
                    .icon(Image::from_bytes(ICON_OFF)?)
                    // Template = macOS renders the alpha mask white on dark menu
                    // bars / black on light ones. Status is shown via opacity.
                    .icon_as_template(true)
                    .tooltip("Hindsight")
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(move |app, event| {
                        on_menu_event(app, event.id().as_ref(), &menu_ui, &menu_starting)
                    })
                    .build(app)?;

                spawn_status_loop(app.handle().clone(), ui, starting.clone());

                // The control center should be up whenever the app is running.
                std::thread::spawn(supervisor::ensure_control_center);
                Ok(())
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Hindsight menu-bar app")
        .run(|_app, event| match event {
            // Dock icon clicked or app relaunched while running → show the
            // controls (the control center), since the menu-bar icon may be
            // hidden by the notch.
            tauri::RunEvent::Reopen { .. } => {
                std::thread::spawn(supervisor::open_control_center);
            }
            // Any exit path: tear down the daemon + control center.
            tauri::RunEvent::Exit => supervisor::teardown(),
            _ => {}
        });
}

fn on_menu_event(app: &AppHandle, id: &str, ui: &Ui, starting: &Arc<AtomicBool>) {
    match id {
        // Lifecycle calls can block (uvx download, model load), so never run
        // them on the menu/main thread.
        "start" => {
            // Paint "starting…" immediately (we're on the main thread here), then
            // run the blocking start off-thread; the poll loop repaints when it
            // finishes or /health comes up.
            starting.store(true, Ordering::SeqCst);
            render(app, ui, false, true, None);
            let starting = starting.clone();
            std::thread::spawn(move || {
                supervisor::daemon_start();
                starting.store(false, Ordering::SeqCst);
            });
        }
        "control" => {
            std::thread::spawn(supervisor::open_control_center);
        }
        "quit" => {
            // Teardown happens in RunEvent::Exit so every quit path (this item,
            // Dock → Quit, Cmd-Q, logout) tears down the daemon + control center.
            let _ = ui.status.set_text("○ Hindsight — shutting down…");
            app.exit(0);
        }
        _ => {}
    }
}

/// Repaint the tray icon, status label, and lifecycle item enablement for the
/// given state. Must run on the main thread (AppKit requirement on macOS).
fn render(app: &AppHandle, ui: &Ui, up: bool, starting: bool, version: Option<&str>) {
    let (label, icon): (String, &[u8]) = if up {
        let ver = version.map(|v| format!(" · API v{v}")).unwrap_or_default();
        (
            format!("● Hindsight — running (:{}){ver}", supervisor::DAEMON_PORT),
            ICON_ON,
        )
    } else if starting {
        ("◐ Hindsight — starting…".to_string(), ICON_STARTING)
    } else {
        ("○ Hindsight — stopped".to_string(), ICON_OFF)
    };
    let _ = ui.status.set_text(&label);
    let _ = ui.start.set_enabled(!up && !starting);
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(img) = Image::from_bytes(icon) {
            let _ = tray.set_icon(Some(img));
        }
    }
}

/// Poll the daemon's health every [`POLL_INTERVAL`] and repaint on any change of
/// (up, starting). UI mutations are marshalled onto the main thread.
fn spawn_status_loop(app: AppHandle, ui: Ui, starting: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut last: Option<(bool, bool, Option<String>)> = None;
        loop {
            let up = supervisor::health_ok();
            let is_starting = starting.load(Ordering::SeqCst);
            let version = if up { supervisor::api_version() } else { None };
            let state = (up, is_starting, version.clone());
            if last.as_ref() != Some(&state) {
                last = Some(state);
                let app2 = app.clone();
                let ui2 = ui.clone();
                let _ = app.run_on_main_thread(move || {
                    render(&app2, &ui2, up, is_starting, version.as_deref())
                });
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

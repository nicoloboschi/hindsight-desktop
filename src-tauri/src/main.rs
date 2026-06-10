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
    stop: MenuItem<Wry>,
    restart: MenuItem<Wry>,
}

fn main() {
    // True between a Start/Restart click and the daemon answering /health (or the
    // start attempt giving up). Shared by the menu handler and the poll loop.
    let starting = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .setup({
            let starting = starting.clone();
            move |app| {
                // No dock icon / app-switcher entry — this lives only in the menu bar.
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);

                let none = None::<&str>;
                let ui = Ui {
                    status: MenuItem::with_id(app, "status", "Checking…", false, none)?,
                    start: MenuItem::with_id(app, "start", "Start", true, none)?,
                    stop: MenuItem::with_id(app, "stop", "Stop", false, none)?,
                    restart: MenuItem::with_id(app, "restart", "Restart", false, none)?,
                };
                let open_ui_i =
                    MenuItem::with_id(app, "open_ui", "Open Control Plane UI", true, none)?;
                let open_cfg_i = MenuItem::with_id(app, "open_config", "Open Config", true, none)?;
                let open_logs_i = MenuItem::with_id(app, "open_logs", "View Logs", true, none)?;
                let docs_i = MenuItem::with_id(app, "docs", "Documentation", true, none)?;
                let quit_i = MenuItem::with_id(app, "quit", "Quit Hindsight Menu", true, none)?;

                let menu = Menu::with_items(
                    app,
                    &[
                        &ui.status,
                        &PredefinedMenuItem::separator(app)?,
                        &open_ui_i,
                        &open_cfg_i,
                        &open_logs_i,
                        &docs_i,
                        &PredefinedMenuItem::separator(app)?,
                        &ui.start,
                        &ui.stop,
                        &ui.restart,
                        &PredefinedMenuItem::separator(app)?,
                        &quit_i,
                    ],
                )?;

                let menu_ui = ui.clone();
                let menu_starting = starting.clone();
                TrayIconBuilder::with_id("main")
                    .icon(Image::from_bytes(ICON_OFF)?)
                    .icon_as_template(false)
                    .tooltip("Hindsight")
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(move |app, event| {
                        on_menu_event(app, event.id().as_ref(), &menu_ui, &menu_starting)
                    })
                    .build(app)?;

                spawn_status_loop(app.handle().clone(), ui, starting.clone());
                Ok(())
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Hindsight menu-bar app")
        .run(|_app, _event| {});
}

fn on_menu_event(app: &AppHandle, id: &str, ui: &Ui, starting: &Arc<AtomicBool>) {
    match id {
        // Lifecycle calls can block (uvx download, model load), so never run
        // them on the menu/main thread.
        "open_ui" => {
            std::thread::spawn(supervisor::open_ui);
        }
        "open_config" => {
            std::thread::spawn(supervisor::open_config);
        }
        "open_logs" => {
            std::thread::spawn(supervisor::open_logs);
        }
        "docs" => supervisor::open_docs(),
        "start" | "restart" => {
            // Paint "starting…" immediately (we're on the main thread here), then
            // run the blocking start off-thread; the poll loop repaints when it
            // finishes or /health comes up.
            starting.store(true, Ordering::SeqCst);
            render(app, ui, false, true, None);
            let starting = starting.clone();
            let restart = id == "restart";
            std::thread::spawn(move || {
                if restart {
                    supervisor::daemon_restart();
                } else {
                    supervisor::daemon_start();
                }
                starting.store(false, Ordering::SeqCst);
            });
        }
        "stop" => {
            starting.store(false, Ordering::SeqCst);
            std::thread::spawn(|| {
                supervisor::daemon_stop();
            });
        }
        "quit" => app.exit(0),
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
    let _ = ui.stop.set_enabled(up);
    let _ = ui.restart.set_enabled(up);
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

// Tray-only app: hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod supervisor;

use std::time::Duration;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Wry,
};

const POLL_INTERVAL: Duration = Duration::from_secs(3);

// Status icons baked into the binary (alpha-mask templates).
const ICON_ON: &[u8] = include_bytes!("../icons/tray-on.png"); // solid = running
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png"); // dim = stopped

/// Handle needed to repaint the tray status line from the poll loop.
#[derive(Clone)]
struct Ui {
    status: MenuItem<Wry>,
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Keep the Dock icon: the menu-bar icon can be hidden by the notch /
            // menu-bar overflow, so the Dock is the reliable surface. Clicking it
            // (RunEvent::Reopen) opens the control center.
            let none = None::<&str>;
            let ui = Ui {
                status: MenuItem::with_id(app, "status", "Checking…", false, none)?,
            };
            let control_i = MenuItem::with_id(app, "control", "Open Control Center", true, none)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Hindsight", true, none)?;

            let menu = Menu::with_items(
                app,
                &[
                    &ui.status,
                    &PredefinedMenuItem::separator(app)?,
                    &control_i,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_i,
                ],
            )?;

            let menu_ui = ui.clone();
            TrayIconBuilder::with_id("main")
                .icon(Image::from_bytes(ICON_OFF)?)
                // Template = macOS renders the alpha mask white on dark menu bars
                // / black on light ones. Status is shown via opacity.
                .icon_as_template(true)
                .tooltip("Hindsight")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| on_menu_event(app, event.id().as_ref(), &menu_ui))
                .build(app)?;

            spawn_status_loop(app.handle().clone(), ui);

            // Make sure our profile exists and the control center is up whenever
            // the app is running. (Profile must exist for the ?profile=desktop
            // deep-link and the :8899 health poll.)
            std::thread::spawn(|| {
                supervisor::ensure_profile();
                supervisor::ensure_control_center();
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Hindsight menu-bar app")
        .run(|_app, event| match event {
            // Dock icon clicked or app relaunched while running → open the control
            // center (the menu-bar icon may be hidden by the notch).
            tauri::RunEvent::Reopen { .. } => {
                std::thread::spawn(supervisor::open_control_center);
            }
            // Any exit path: tear down the daemon + control center.
            tauri::RunEvent::Exit => supervisor::teardown(),
            _ => {}
        });
}

fn on_menu_event(app: &AppHandle, id: &str, ui: &Ui) {
    match id {
        "control" => {
            // Blocks (uvx / control start), so off the menu/main thread.
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

/// Repaint the tray icon + status label for the given state. Must run on the
/// main thread (AppKit requirement on macOS).
fn render(app: &AppHandle, ui: &Ui, up: bool, version: Option<&str>) {
    let (label, icon): (String, &[u8]) = if up {
        let ver = version.map(|v| format!(" · API v{v}")).unwrap_or_default();
        (
            format!("● Hindsight — running (:{}){ver}", supervisor::DAEMON_PORT),
            ICON_ON,
        )
    } else {
        ("○ Hindsight — stopped".to_string(), ICON_OFF)
    };
    let _ = ui.status.set_text(&label);
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(img) = Image::from_bytes(icon) {
            let _ = tray.set_icon(Some(img));
        }
    }
}

/// Poll the daemon's health every [`POLL_INTERVAL`] and repaint on any change of
/// (up, version). UI mutations are marshalled onto the main thread.
fn spawn_status_loop(app: AppHandle, ui: Ui) {
    std::thread::spawn(move || {
        let mut last: Option<(bool, Option<String>)> = None;
        loop {
            let up = supervisor::health_ok();
            let version = if up { supervisor::api_version() } else { None };
            let state = (up, version.clone());
            if last.as_ref() != Some(&state) {
                last = Some(state);
                let app2 = app.clone();
                let ui2 = ui.clone();
                let _ = app
                    .run_on_main_thread(move || render(&app2, &ui2, up, version.as_deref()));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

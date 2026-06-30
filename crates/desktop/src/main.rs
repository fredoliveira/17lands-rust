// Menu-bar (notification-area) desktop app for the 17Lands MTGA log client.
// On Windows, don't pop a console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod logbridge;
mod observer;
mod state;

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use seventeenlands_core::{api_client, config};
use state::AppState;

fn main() {
    // Install the log->webview bridge before anything logs.
    logbridge::WebviewLogger::install();

    // Dev safety: never POST to the live API during development. Set SEVENTEENLANDS_HOST
    // (e.g. the local oracle mock) to override the default live host.
    let host = std::env::var("SEVENTEENLANDS_HOST")
        .unwrap_or_else(|_| api_client::DEFAULT_HOST.to_string());

    tauri::Builder::default()
        .manage(AppState::new(host))
        .invoke_handler(tauri::generate_handler![
            commands::token_present,
            commands::save_token,
            commands::get_status,
            commands::start_following,
            commands::stop_following,
            commands::recent_logs,
            commands::set_log_path,
        ])
        .setup(|app| {
            logbridge::attach(app.handle().clone());

            // Menu-bar app: no dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            build_tray(app.handle())?;

            // First run: if no token yet, open the window on Settings for onboarding.
            // Otherwise start following immediately.
            let following_started =
                config::read_toml_token().is_some() && app.state::<AppState>().start().is_ok();

            if !following_started {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                let _ = app.emit("show-settings", ());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window only hides it; the app keeps living in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running 17Lands desktop app");
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Log Window", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Start / Stop Following", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit 17Lands", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &settings, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("17Lands")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone()).icon_as_template(true);
    }

    builder.build(app)?;
    Ok(())
}

fn handle_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "show" => show_window(app),
        "settings" => {
            show_window(app);
            let _ = app.emit("show-settings", ());
        }
        "toggle" => {
            let state = app.state::<AppState>();
            if state.is_following() {
                state.stop();
            } else {
                let _ = state.start();
            }
            let _ = app.emit("status-changed", ());
        }
        "quit" => {
            app.state::<AppState>().stop();
            app.exit(0);
        }
        _ => {}
    }
}

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

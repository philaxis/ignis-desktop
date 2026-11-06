#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod frame;
mod snap;
mod state;
mod urls;
mod windows;

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};

use crate::{
    config::Config,
    frame::FrameAccess,
    state::AppState,
    windows::{
        apply_settings, open_new_window, open_setup_window, quit, restore_windows, OpenOptions,
    },
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    server_url: String,
    vault: String,
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    let config = state.config();
    Settings {
        server_url: config.server_url.clone().unwrap_or_default(),
        vault: config.vault.clone().unwrap_or_default(),
    }
}

/// `async` so it runs off the UI thread: it creates windows.
#[tauri::command]
async fn save_settings(app: AppHandle, server_url: String, vault: String) -> Result<(), String> {
    apply_settings(&app, &server_url, &vault)
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let new_window = MenuItemBuilder::with_id("new-window", "New window").build(app)?;
    let change_server =
        MenuItemBuilder::with_id("change-server", "Change server address…").build(app)?;
    let system_title_bar =
        CheckMenuItemBuilder::with_id("system-title-bar", "Use system title bar")
            .checked(app.state::<AppState>().config().system_title_bar)
            .build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&new_window)
        .item(&change_server)
        .item(&system_title_bar)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&quit_item)
        .build()?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("Ignis Desktop")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "new-window" => {
                let app = app.clone();
                std::thread::spawn(move || open_new_window(&app, OpenOptions::default()));
            }
            "change-server" => open_setup_window(app),
            // The item flips its own check mark when clicked; windows opened
            // from now on follow it.
            "system-title-bar" => {
                let state = app.state::<AppState>();
                state.config().system_title_bar = system_title_bar.is_checked().unwrap_or(false);
                state.flush();
            }
            "quit" => quit(app),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Starting the app again opens another window instead of a second process.
            let app = app.clone();
            std::thread::spawn(move || open_new_window(&app, OpenOptions::default()));
        }))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            frame::frame_start_dragging,
            frame::frame_minimize,
            frame::frame_toggle_maximize,
            frame::frame_close,
            frame::frame_state,
            frame::frame_snap_rect,
        ])
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("config.json");
            let config = Config::load(&path).unwrap_or_else(|e| {
                eprintln!("ignis-desktop: ignoring unreadable {}: {e}", path.display());
                Config::default()
            });
            let configured = config.server_url.is_some();
            app.manage(AppState::new(path, config));
            app.manage(FrameAccess::default());

            if let Err(e) = build_tray(app.handle()) {
                eprintln!("ignis-desktop: no tray icon: {e}");
            }
            if configured {
                restore_windows(app.handle());
            } else {
                open_setup_window(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build the Ignis Desktop app")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.quitting.store(true, Ordering::SeqCst);
                    state.flush();
                }
            }
        });
}

pub mod commands;
pub mod config;
pub mod launcher;
pub mod platform;
pub mod ready;
pub mod tray;

use config::AppConfig;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, WindowEvent};

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub path: PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("failed to resolve app data dir");
            let path = dir.join("config.json");
            let cfg = AppConfig::load(&path);
            app.manage(AppState { config: Mutex::new(cfg), path });
            tray::setup(app.handle())?;
            #[cfg(debug_assertions)]
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::list_subdirs,
            commands::launch_project_cmd,
            commands::launch_group_cmd,
            commands::run_step,
            commands::open_dir,
            commands::export_config_to,
            commands::import_config_from,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

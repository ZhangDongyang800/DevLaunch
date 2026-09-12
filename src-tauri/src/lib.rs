pub mod commands;
pub mod config;
pub mod detect;
pub mod hotkey;
pub mod launcher;
pub mod platform;
pub mod tray;

use config::AppConfig;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub path: PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("failed to resolve app data dir");
            let path = dir.join("config.json");
            let loaded = AppConfig::load_diagnostic(&path);
            let mut cfg = loaded.config;
            if let Ok(enabled) = app.autolaunch().is_enabled() {
                cfg.settings.autostart = enabled;
            }
            if let Some(backup) = &loaded.corrupt_backup {
                launcher::notify(
                    app.handle(),
                    format!("配置文件损坏，已备份到 {}，并恢复默认配置", backup.display()),
                );
            }
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
            commands::launch_item_cmd,
            commands::open_dir,
            commands::export_config_to,
            commands::import_config_from,
            commands::export_project,
            commands::export_project_file,
            commands::read_project_template,
            commands::get_autostart,
            commands::set_autostart,
            commands::detect_project,
            commands::scan_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

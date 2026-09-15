pub mod commands;
pub mod config;
pub mod detect;
pub mod git;
pub mod hotkey;
pub mod launcher;
pub mod platform;
pub mod tray;

use config::AppConfig;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub path: PathBuf,
    pub hotkey: Mutex<Option<String>>,
}

fn toggle_palette(app: &AppHandle) {
    let Some(win) = app.get_webview_window("palette") else { return };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
    } else {
        let _ = win.show();
        let _ = win.set_focus();
        let _ = app.emit_to("palette", "palette-shown", ());
    }
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
            let hotkey_spec = cfg.settings.hotkey.clone();
            app.manage(AppState { config: Mutex::new(cfg), path, hotkey: Mutex::new(Some(hotkey_spec.clone())) });
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, _shortcut, event| {
                        if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            toggle_palette(app);
                        }
                    })
                    .build(),
            )?;
            let mut active = hotkey_spec.clone();
            if crate::hotkey::register(app.handle(), &hotkey_spec).is_err() {
                active = crate::config::DEFAULT_HOTKEY.to_string();
                if crate::hotkey::register(app.handle(), &active).is_err() {
                    launcher::notify(app.handle(), "全局快捷键注册失败，搜索面板仍可从托盘打开主窗口后使用设置重新配置".into());
                } else {
                    launcher::notify(app.handle(), format!("快捷键 {hotkey_spec} 注册失败，已回退默认 {}", crate::config::DEFAULT_HOTKEY));
                }
            }
            if let Ok(mut g) = app.state::<AppState>().hotkey.lock() {
                *g = Some(active);
            }
            tray::setup(app.handle())?;
            #[cfg(debug_assertions)]
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "palette" {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                    WindowEvent::Focused(false) => {
                        let _ = window.hide();
                    }
                    _ => {}
                }
                return;
            }
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
            commands::set_hotkey,
            commands::hide_palette,
            commands::git_statuses,
            commands::git_log,
            commands::git_commit_detail,
            commands::git_branches,
            commands::git_file_diff,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

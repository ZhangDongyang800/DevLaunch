use crate::commands;
use crate::launcher;
use crate::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

pub fn setup(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = build_menu(app)?;
    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("DevLaunch")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().0.clone()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)
}

/// Windows 菜单把 & 当加速键，显示字面量需转义。
pub fn menu_label(name: &str) -> String {
    name.replace('&', "&&")
}

pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    // 锁被污染（别处 panic 过）时不能 panic：release 构建是 panic=abort，会直接杀掉进程。
    let cfg = app
        .state::<AppState>()
        .config
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    let menu = Menu::new(app)?;
    for p in &cfg.projects {
        let sub = Submenu::with_id(app, format!("proj-{}", p.id), menu_label(&p.name), true)?;
        sub.append(&MenuItem::with_id(app, format!("launch:{}", p.id), "启动", true, None::<&str>)?)?;
        sub.append(&MenuItem::with_id(app, format!("open:{}", p.id), "打开目录", true, None::<&str>)?)?;
        menu.append(&sub)?;
    }
    if !cfg.projects.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(app, "show", "打开 DevLaunch", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?)?;
    Ok(menu)
}

pub fn rebuild(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        eprintln!("tray rebuild skipped: tray icon not found");
        return;
    };
    match build_menu(app) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                eprintln!("tray set_menu failed: {e}");
            }
        }
        Err(e) => eprintln!("tray menu rebuild failed: {e}"),
    }
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn handle_menu(app: &AppHandle, id: String) {
    match id.as_str() {
        "show" => show_main_window(app),
        "quit" => app.exit(0),
        _ => {}
    }
    if let Some(project_id) = id.strip_prefix("launch:") {
        let app = app.clone();
        let project_id = project_id.to_string();
        std::thread::spawn(move || {
            let Ok(cfg) = app.state::<AppState>().config.lock().map(|g| g.clone()) else { return };
            if let Err(e) = launcher::launch_project(&app, &cfg, &project_id) {
                eprintln!("launch failed: {e}");
                show_main_window(&app);
                let _ = app.emit("launch-error", e);
            } else {
                commands::record_launch(&app, &project_id);
            }
        });
    }
    if let Some(project_id) = id.strip_prefix("open:") {
        // 托盘点击没有任何窗口反馈，失败必须用系统通知说出来，否则就是「点了没反应」。
        let cfg = match app.state::<AppState>().config.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        let Some(p) = cfg.projects.iter().find(|p| p.id == project_id) else { return };
        let root_dir = p.root_dir.clone();
        if let Err(e) = commands::open_dir(root_dir.clone()) {
            launcher::notify(app, format!("打开目录失败：{e}（{root_dir}）"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_label_escapes_ampersand() {
        assert_eq!(menu_label("A&B"), "A&&B");
        assert_eq!(menu_label("plain"), "plain");
    }
}

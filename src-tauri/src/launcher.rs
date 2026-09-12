use crate::config::{AppConfig, Item, Project};
use crate::platform::{self, LaunchMode, PaneSpec};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

pub fn notify(app: &AppHandle, body: String) {
    let _ = app.notification().builder().title("DevLaunch").body(&body).show();
}

fn main_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

pub fn resolve_work_dir(root_dir: &str, work_dir: &Option<String>) -> PathBuf {
    let p = match work_dir {
        Some(w) if !w.trim().is_empty() => {
            let p = Path::new(w);
            if p.is_absolute() { p.to_path_buf() } else { Path::new(root_dir).join(p) }
        }
        _ => Path::new(root_dir).to_path_buf(),
    };
    p.components().collect()
}

pub fn launch_project(app: &AppHandle, cfg: &AppConfig, project_id: &str) -> Result<(), String> {
    let project = cfg.projects.iter().find(|p| p.id == project_id)
        .ok_or_else(|| format!("未找到项目 {project_id}"))?;
    launch_items(app, project, &project.items)
}

pub fn launch_item(app: &AppHandle, cfg: &AppConfig, project_id: &str, item_id: &str) -> Result<(), String> {
    let project = cfg.projects.iter().find(|p| p.id == project_id)
        .ok_or_else(|| format!("未找到项目 {project_id}"))?;
    let item = project.items.iter().find(|i| i.id == item_id)
        .ok_or_else(|| format!("未找到启动项 {item_id}"))?;
    launch_items(app, project, std::slice::from_ref(item))
}

pub fn launch_items(app: &AppHandle, project: &Project, items: &[Item]) -> Result<(), String> {
    let panes = build_panes(project, items).map_err(|e| { notify(app, e.clone()); e })?;
    let mode = platform::spawn_panes(&project.name, &panes)
        .map_err(|e| { let m = format!("启动失败：{e}"); notify(app, m.clone()); m })?;
    let hidden = !main_window_visible(app);
    match (mode, hidden) {
        (LaunchMode::Fallback, true) => notify(
            app,
            format!("未检测到 Windows Terminal，已用 {} 个独立终端窗口启动「{}」", panes.len(), project.name),
        ),
        (LaunchMode::Fallback, false) => notify(
            app,
            format!("未检测到 Windows Terminal，已降级为 {} 个独立终端窗口", panes.len()),
        ),
        (LaunchMode::WindowsTerminal, true) => {
            notify(app, format!("已启动「{}」（{} 个窗格）", project.name, panes.len()));
        }
        (LaunchMode::WindowsTerminal, false) => {}
    }
    Ok(())
}

pub fn build_panes(project: &Project, items: &[Item]) -> Result<Vec<PaneSpec>, String> {
    if items.is_empty() {
        return Err("没有可启动的启动项".into());
    }
    let mut panes = Vec::with_capacity(items.len());
    for item in items {
        let wd = resolve_work_dir(&project.root_dir, &item.work_dir);
        if !wd.is_dir() {
            return Err(format!("「{}」目录不存在：{}", item.name, wd.display()));
        }
        panes.push(PaneSpec {
            title: item.name.clone(),
            work_dir: wd,
            shell: item.shell,
            command: item.command.clone(),
        });
    }
    Ok(panes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Shell;

    fn project() -> (tempfile::TempDir, Project) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("backend")).unwrap();
        let p = Project {
            id: "p1".into(), name: "X".into(), root_dir: dir.path().to_string_lossy().to_string(),
            favorite: false, last_launched_at: None,
            items: vec![Item { id: "i1".into(), name: "后端".into(), work_dir: Some("backend".into()), shell: Shell::Cmd, command: "python app.py".into() }],
        };
        (dir, p)
    }

    #[test]
    fn build_panes_resolves_workdir_and_maps_fields() {
        let (_dir, p) = project();
        let panes = build_panes(&p, &p.items).unwrap();
        assert_eq!(panes[0].work_dir, Path::new(&p.root_dir).join("backend"));
        assert_eq!(panes[0].title, "后端");
    }

    #[test]
    fn build_panes_errors_on_missing_dir() {
        let (_dir, mut p) = project();
        p.items[0].work_dir = Some("nope-xyz".into());
        assert!(build_panes(&p, &p.items).unwrap_err().contains("目录不存在"));
    }

    #[test]
    fn resolve_work_dir_strips_trailing_separators() {
        let cases = [
            (resolve_work_dir(r"D:\proj\", &None), r"D:\proj"),
            (resolve_work_dir(r"D:\proj", &Some(r"backend\".into())), r"D:\proj\backend"),
            (resolve_work_dir(r"D:\proj", &Some(r"D:\abs\dir\".into())), r"D:\abs\dir"),
            (resolve_work_dir(r"D:\", &None), r"D:\"),
        ];
        for (got, want) in cases {
            // Path eq ignores trailing separators; the command line uses display(), so assert that.
            assert_eq!(got.to_string_lossy(), want);
            assert_eq!(got, PathBuf::from(want));
        }
    }
}

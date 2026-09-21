use crate::config::{AppConfig, Item, Project};
use crate::platform::{self, LaunchMode, PaneSpec};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

/// 发系统通知。
///
/// 失败**不能静默**：未注册 AUMID 时（便携 exe、非安装目录运行、`tauri dev`）
/// 系统会把通知直接丢掉。那种情况下用户什么也看不到——把失败写进日志，
/// 至少留下一条可诊断的痕迹。
pub fn notify(app: &AppHandle, body: String) {
    if let Err(e) = app.notification().builder().title("DevLaunch").body(&body).show() {
        crate::diag::warn(format!("系统通知发送失败（{e}）：{body}"));
    }
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
    let panes = build_panes(project, items).inspect_err(|e| notify(app, e.clone()))?;
    let mode = platform::spawn_panes(&project.name, &panes)
        .map_err(|e| { let m = format!("启动失败：{e}"); notify(app, m.clone()); m })?;
    report_launch(app, &project.name, panes.len(), mode);
    Ok(())
}

/// 在指定工作目录（按任务环境 = worktree 路径）里启动项目的全部启动项。
pub fn launch_worktree(
    app: &AppHandle,
    cfg: &AppConfig,
    project_id: &str,
    branch: &str,
) -> Result<(), String> {
    let project = cfg.projects.iter().find(|p| p.id == project_id)
        .ok_or_else(|| format!("未找到项目 {project_id}"))?;
    let settings = project
        .worktree
        .clone()
        .ok_or_else(|| format!("项目「{}」未启用按任务环境", project.name))?;
    let repo = crate::commands::project_dir(cfg, project_id)?;
    let list = crate::worktree::list(&repo)?;
    let dir = crate::worktree::resolve_worktree_path(&list, branch)?;
    let env = crate::worktree::pane_env(&settings, branch, &dir.to_string_lossy())?;
    let panes = build_panes_in(&project.items, &dir, &env)
        .inspect_err(|e| notify(app, e.clone()))?;
    let label = format!("{} · {}", project.name, branch.trim());
    let mode = platform::spawn_panes(&label, &panes)
        .map_err(|e| { let m = format!("启动失败：{e}"); notify(app, m.clone()); m })?;
    report_launch(app, &label, panes.len(), mode);
    Ok(())
}

/// 主窗口可见时只发降级通知、隐藏时（托盘启动）才发成功通知。
fn report_launch(app: &AppHandle, label: &str, panes: usize, mode: LaunchMode) {
    if main_window_visible(app) {
        if mode == LaunchMode::Fallback {
            notify(app, format!("未检测到 Windows Terminal，已降级为 {panes} 个独立终端窗口"));
        }
        return;
    }
    match mode {
        LaunchMode::Fallback => notify(
            app,
            format!("未检测到 Windows Terminal，已用 {panes} 个独立终端窗口启动「{label}」"),
        ),
        LaunchMode::WindowsTerminal => {
            notify(app, format!("已启动「{label}」（{panes} 个窗格）"));
        }
    }
}

pub fn build_panes(project: &Project, items: &[Item]) -> Result<Vec<PaneSpec>, String> {
    build_panes_in(items, Path::new(&project.root_dir), &[])
}

/// `base_dir` 覆盖项目的根目录：按任务环境把相对 workDir 解析到 worktree 里，
/// 绝对 workDir 保持原样（仓库外的共享目录不随环境漂移）。
pub fn build_panes_in(
    items: &[Item],
    base_dir: &Path,
    env: &[(String, String)],
) -> Result<Vec<PaneSpec>, String> {
    if items.is_empty() {
        return Err("没有可启动的启动项".into());
    }
    for (key, value) in env {
        crate::worktree::validate_env_pair(key, value)
            .map_err(|e| format!("环境注入失败：{e}"))?;
    }
    let root_dir = base_dir.to_string_lossy().to_string();
    let mut panes = Vec::with_capacity(items.len());
    for item in items {
        let label = if item.name.trim().is_empty() { "未命名启动项" } else { item.name.as_str() };
        // 空命令会开出一个「什么都不做」的终端窗口：看起来启动成功了，其实没有。
        // 在计划阶段就拒绝并指名道姓，比让用户对着空白窗格猜要好。
        if item.command.trim().is_empty() {
            return Err(format!("「{label}」还没有填写命令，请先在编辑器里补上"));
        }
        let wd = resolve_work_dir(&root_dir, &item.work_dir);
        if !wd.is_dir() {
            return Err(format!("「{label}」目录不存在：{}", wd.display()));
        }
        panes.push(PaneSpec {
            title: item.name.clone(),
            work_dir: wd,
            shell: item.shell,
            command: item.command.clone(),
            env: env.to_vec(),
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
            worktree: None,
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

    /// 空命令会开出一个什么都不做的窗口，看起来像"启动成功了但没反应"。
    #[test]
    fn build_panes_errors_on_empty_command() {
        let (_dir, mut p) = project();
        p.items[0].command = "   \n\t ".into();
        let err = build_panes(&p, &p.items).unwrap_err();
        assert!(err.contains("还没有填写命令"), "{err}");
        // 报错要指名道姓，否则用户不知道是哪个启动项
        assert!(err.contains("后端"), "{err}");
    }

    #[test]
    fn build_panes_labels_unnamed_item_in_error() {
        let (_dir, mut p) = project();
        p.items[0].name = "  ".into();
        p.items[0].command = String::new();
        let err = build_panes(&p, &p.items).unwrap_err();
        assert!(err.contains("未命名启动项"), "{err}");
    }

    #[test]
    fn build_panes_in_redirects_relative_dirs_and_carries_env() {
        let (_dir, p) = project();
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir(base.path().join("backend")).unwrap();
        let mut with_abs = p.clone();
        with_abs.items.push(Item {
            id: "i2".into(),
            name: "共享".into(),
            work_dir: Some(base.path().to_string_lossy().to_string()),
            shell: Shell::Cmd,
            command: "echo hi".into(),
        });
        let env = vec![("PORT".to_string(), "5173".to_string())];
        let panes = build_panes_in(&with_abs.items, base.path(), &env).unwrap();
        assert_eq!(panes[0].work_dir, base.path().join("backend"));
        assert_eq!(panes[1].work_dir, base.path());
        assert!(panes.iter().all(|pane| pane.env == env));
        assert!(build_panes_in(&with_abs.items, base.path(), &[("PORT".into(), "5;1|7".into())])
            .unwrap_err()
            .contains("环境注入失败"));
        assert!(build_panes_in(&[], base.path(), &env).is_err());
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

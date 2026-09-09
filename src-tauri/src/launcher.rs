use crate::config::{AppConfig, Group, Project, Step};
use crate::platform;
use crate::ready;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn notify(app: &AppHandle, body: String) {
    let _ = app
        .notification()
        .builder()
        .title("DevLaunch")
        .body(&body)
        .show();
}

pub fn resolve_work_dir(root_dir: &str, work_dir: &Option<String>) -> PathBuf {
    match work_dir {
        Some(w) if !w.trim().is_empty() => {
            let p = Path::new(w);
            if p.is_absolute() { p.to_path_buf() } else { Path::new(root_dir).join(p) }
        }
        _ => Path::new(root_dir).to_path_buf(),
    }
}

pub fn launch_project(app: &AppHandle, cfg: &AppConfig, project_id: &str) -> Result<(), String> {
    let project = cfg
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("未找到项目 {project_id}"))?;
    for group in &project.groups {
        launch_group_steps(app, project, group, cfg.settings.ready_timeout_sec)?;
    }
    notify(app, format!("项目「{}」启动完成", project.name));
    Ok(())
}

pub fn launch_group(app: &AppHandle, cfg: &AppConfig, project_id: &str, group_id: &str) -> Result<(), String> {
    let project = cfg
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("未找到项目 {project_id}"))?;
    let group = project
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("未找到分组 {group_id}"))?;
    launch_group_steps(app, project, group, cfg.settings.ready_timeout_sec)
}

pub fn run_step(app: &AppHandle, cfg: &AppConfig, project_id: &str, group_id: &str, step_id: &str) -> Result<(), String> {
    let project = cfg
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("未找到项目 {project_id}"))?;
    let group = project
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("未找到分组 {group_id}"))?;
    let step = group
        .steps
        .iter()
        .find(|s| s.id == step_id)
        .ok_or_else(|| format!("未找到步骤 {step_id}"))?;
    spawn_step(app, project, step)
}

fn launch_group_steps(app: &AppHandle, project: &Project, group: &Group, default_timeout: u64) -> Result<(), String> {
    for step in &group.steps {
        spawn_step(app, project, step)?;
        if let Err(e) = ready::wait_ready(&step.ready_condition, default_timeout) {
            let msg = format!(
                "「{}」未就绪：{}（超时 {} 秒），已停止后续启动",
                step.name, e.description, effective_timeout(&step.ready_condition, default_timeout)
            );
            notify(app, msg.clone());
            return Err(msg);
        }
    }
    Ok(())
}

fn effective_timeout(cond: &crate::config::ReadyCondition, default_timeout: u64) -> u64 {
    use crate::config::ReadyCondition as RC;
    match cond {
        RC::Port { timeout_sec, .. } | RC::Process { timeout_sec, .. } if *timeout_sec > 0 => *timeout_sec,
        RC::Delay { seconds } => *seconds,
        _ => default_timeout,
    }
}

fn spawn_step(app: &AppHandle, project: &Project, step: &Step) -> Result<(), String> {
    let work_dir = resolve_work_dir(&project.root_dir, &step.work_dir);
    let wd = work_dir.to_string_lossy().to_string();
    if !work_dir.is_dir() {
        let msg = format!("「{}」目录不存在：{}", step.name, wd);
        notify(app, msg.clone());
        return Err(msg);
    }
    platform::spawn(step.terminal, &wd, &step.command).map_err(|e| {
        let msg = format!("「{}」启动失败：{e}", step.name);
        notify(app, msg.clone());
        msg
    })
}

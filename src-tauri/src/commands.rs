use crate::config::{AppConfig, ProjectTemplate};
use crate::launcher;
use crate::tray;
use crate::AppState;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Group, Project, Step, Terminal};

    #[test]
    fn subdirs_lists_only_directories_sorted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("web")).unwrap();
        std::fs::create_dir(dir.path().join("server")).unwrap();
        std::fs::write(dir.path().join("file.txt"), "x").unwrap();
        let got = subdirs(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(got, vec!["server", "web"]);
    }

    #[test]
    fn subdirs_errors_on_missing_path() {
        assert!(subdirs(r"Z:\definitely-missing-xyz-12345").is_err());
    }

    fn sample_project(id: &str) -> Project {
        Project {
            id: id.into(),
            name: "PVDS".into(),
            root_dir: r"D:\Projects\PVDS".into(),
            groups: vec![Group {
                id: "g1".into(),
                name: "默认".into(),
                terminal: Terminal::Cmd,
                steps: vec![Step {
                    id: "s1".into(),
                    name: "server".into(),
                    work_dir: Some("server".into()),
                    terminal: Terminal::Cmd,
                    command: "python app.py".into(),
                    ready_condition: crate::config::ReadyCondition::Immediate,
                }],
            }],
        }
    }

    #[test]
    fn export_project_writes_template_without_root_dir() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(sample_project("p1"));
        cfg.projects.push(sample_project("p2"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pvds.json");
        export_project_to(&cfg, "p1", &path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"name\": \"PVDS\""));
        assert!(!text.contains("rootDir"));
        assert!(!text.contains("p2"));
        let tpl = read_template(&path).unwrap();
        assert_eq!(tpl.groups.len(), 1);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn export_project_errors_on_missing_project() {
        let cfg = AppConfig::default();
        let dir = tempfile::tempdir().unwrap();
        assert!(export_project_to(&cfg, "nope", &dir.path().join("x.json")).is_err());
    }

    #[test]
    fn read_template_rejects_bad_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "{ not valid json").unwrap();
        assert!(read_template(&path).is_err());
    }
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: AppConfig) -> Result<(), String> {
    {
        let mut guard = state.config.lock().unwrap();
        *guard = config;
        guard.save(&state.path)?;
    }
    tray::rebuild(&app);
    Ok(())
}

#[tauri::command]
pub fn launch_project_cmd(app: AppHandle, state: State<AppState>, project_id: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    std::thread::spawn(move || {
        let result = launcher::launch_project(&app, &cfg, &project_id);
        let _ = app.emit("launch-result", result.err());
    });
    Ok(())
}

#[tauri::command]
pub fn launch_group_cmd(
    app: AppHandle,
    state: State<AppState>,
    project_id: String,
    group_id: String,
) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    std::thread::spawn(move || {
        let result = launcher::launch_group(&app, &cfg, &project_id, &group_id);
        let _ = app.emit("launch-result", result.err());
    });
    Ok(())
}

#[tauri::command]
pub fn run_step(
    app: AppHandle,
    state: State<AppState>,
    project_id: String,
    group_id: String,
    step_id: String,
) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    launcher::run_step(&app, &cfg, &project_id, &group_id, &step_id)
}

#[tauri::command]
pub fn open_dir(path: String) -> Result<(), String> {
    if !Path::new(&path).is_dir() {
        return Err(format!("目录不存在：{path}"));
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        return Err("open_dir 仅支持 Windows（v1）".into());
    }
    Ok(())
}

#[tauri::command]
pub fn export_project(state: State<AppState>, project_id: String, path: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    export_project_to(&cfg, &project_id, Path::new(&path))
}

pub fn export_project_to(cfg: &AppConfig, project_id: &str, path: &Path) -> Result<(), String> {
    let project = cfg
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("项目不存在：{project_id}"))?;
    crate::config::save_json(&ProjectTemplate::from_project(project), path)
}

#[tauri::command]
pub fn read_project_template(path: String) -> Result<ProjectTemplate, String> {
    read_template(Path::new(&path))
}

pub fn read_template(path: &Path) -> Result<ProjectTemplate, String> {
    ProjectTemplate::load(path)
}

#[tauri::command]
pub fn export_config_to(state: State<AppState>, path: String) -> Result<(), String> {
    state.config.lock().unwrap().save(Path::new(&path))
}

#[tauri::command]
pub fn import_config_from(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    let text = fs::read_to_string(&path).map_err(|e| format!("读取失败：{e}"))?;
    let cfg: AppConfig =
        serde_json::from_str(&text).map_err(|e| format!("配置文件格式错误：{e}"))?;
    let cfg = AppConfig::migrate_if_needed(cfg);
    {
        let mut guard = state.config.lock().unwrap();
        *guard = cfg;
        guard.save(&state.path)?;
    }
    tray::rebuild(&app);
    Ok(())
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

pub fn subdirs(path: &str) -> Result<Vec<String>, String> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            dirs.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    dirs.sort();
    Ok(dirs)
}

#[tauri::command]
pub fn list_subdirs(path: String) -> Result<Vec<String>, String> {
    subdirs(&path)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    if enabled {
        auto.enable().map_err(|e| e.to_string())
    } else {
        auto.disable().map_err(|e| e.to_string())
    }
}

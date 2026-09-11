use crate::config::{AppConfig, ProjectTemplate};
use crate::launcher;
use crate::tray;
use crate::AppState;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Item, Project, Shell};

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
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("definitely-missing-xyz-12345");
        assert!(subdirs(missing.to_str().unwrap()).is_err());
    }

    #[test]
    fn validate_rejects_empty_project_id() {
        let mut cfg = AppConfig::default();
        let mut p = sample_project("");
        p.id = "".into();
        cfg.projects.push(p);
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.contains("ID"), "{err}");
    }

    #[test]
    fn validate_rejects_duplicate_project_ids() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(sample_project("p1"));
        cfg.projects.push(sample_project("p1"));
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.contains("重复"), "{err}");
    }

    #[test]
    fn validate_rejects_duplicate_item_ids() {
        let mut cfg = AppConfig::default();
        let mut p = sample_project("p1");
        p.items.push(p.items[0].clone());
        cfg.projects.push(p);
        let err = validate_config(&cfg).unwrap_err();
        assert!(err.contains("重复"), "{err}");
    }

    #[test]
    fn apply_config_rolls_back_memory_on_save_failure() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, "x").unwrap();
        let path = blocker.join("nested").join("config.json");
        let initial = AppConfig::default();
        let state = crate::AppState {
            config: std::sync::Mutex::new(initial.clone()),
            path,
        };
        let mut next = AppConfig::default();
        next.projects.push(sample_project("p1"));
        assert!(apply_config(&state, next).is_err());
        assert_eq!(*state.config.lock().unwrap(), initial);
    }

    #[test]
    fn backup_config_file_copies_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{}").unwrap();
        let bak = backup_config_file(&path).expect("backup created");
        assert!(bak.is_file());
        assert!(path.is_file());
        assert!(bak.file_name().unwrap().to_string_lossy().contains("config.json.bak-"));
    }

    fn sample_project(id: &str) -> Project {
        Project {
            id: id.into(),
            name: "PVDS".into(),
            root_dir: r"D:\Projects\PVDS".into(),
            items: vec![Item {
                id: "i1".into(),
                name: "server".into(),
                work_dir: Some("server".into()),
                shell: Shell::Cmd,
                command: "python app.py".into(),
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
        assert_eq!(tpl.items.len(), 1);
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

    #[test]
    fn export_project_file_writes_to_root() {
        let mut cfg = AppConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let mut p = sample_project("p1");
        p.root_dir = dir.path().to_string_lossy().to_string();
        cfg.projects.push(p);
        let path = Path::new(&cfg.projects[0].root_dir).join("devlaunch.json");
        export_project_to(&cfg, "p1", &path).unwrap();
        assert!(path.is_file());
    }
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: AppConfig) -> Result<(), String> {
    apply_config(&state, config)?;
    tray::rebuild(&app);
    Ok(())
}

/// 唯一配置写入口：先校验，再落盘；落盘失败时回滚内存，避免内存/磁盘分叉。
fn apply_config(state: &AppState, config: AppConfig) -> Result<(), String> {
    validate_config(&config)?;
    let mut guard = state.config.lock().map_err(|_| "配置状态不可用".to_string())?;
    let previous = guard.clone();
    *guard = config;
    if let Err(e) = guard.save(&state.path) {
        *guard = previous;
        return Err(e);
    }
    Ok(())
}

pub fn validate_config(cfg: &AppConfig) -> Result<(), String> {
    let mut project_ids = HashSet::new();
    for project in &cfg.projects {
        if project.id.trim().is_empty() {
            return Err(format!("项目「{}」缺少 ID，请重新创建该项目", project.name));
        }
        if !project_ids.insert(project.id.as_str()) {
            return Err(format!("项目 ID 重复：{}", project.id));
        }
        let mut item_ids = HashSet::new();
        for item in &project.items {
            if item.id.trim().is_empty() {
                return Err(format!("项目「{}」存在缺少 ID 的启动项", project.name));
            }
            if !item_ids.insert(item.id.as_str()) {
                return Err(format!("项目「{}」的启动项 ID 重复，请重新导入该启动项", project.name));
            }
        }
    }
    Ok(())
}

fn backup_config_file(path: &Path) -> Option<PathBuf> {
    if !path.is_file() {
        return None;
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let bak = path.with_extension(format!("json.bak-{ts}"));
    fs::copy(path, &bak).ok().map(|_| bak)
}

#[tauri::command]
pub fn launch_project_cmd(app: AppHandle, state: State<AppState>, project_id: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    launcher::launch_project(&app, &cfg, &project_id)
}

#[tauri::command]
pub fn launch_item_cmd(app: AppHandle, state: State<AppState>, project_id: String, item_id: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    launcher::launch_item(&app, &cfg, &project_id, &item_id)
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
        return Err("open_dir 仅支持 Windows".into());
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
pub fn export_project_file(state: State<AppState>, project_id: String) -> Result<String, String> {
    let cfg = state.config.lock().unwrap().clone();
    let project = cfg.projects.iter().find(|p| p.id == project_id)
        .ok_or_else(|| format!("项目不存在：{project_id}"))?;
    if project.root_dir.trim().is_empty() {
        return Err("项目未设置根目录".into());
    }
    let path = Path::new(&project.root_dir).join("devlaunch.json");
    export_project_to(&cfg, &project_id, &path)?;
    Ok(path.to_string_lossy().to_string())
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
    let cfg = crate::config::parse_config(&text).map_err(|e| format!("配置文件格式错误：{e}"))?;
    backup_config_file(&state.path);
    apply_config(&state, cfg)?;
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
pub fn set_autostart(app: AppHandle, state: State<AppState>, enabled: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    if enabled {
        auto.enable().map_err(|e| e.to_string())?;
    } else {
        auto.disable().map_err(|e| e.to_string())?;
    }
    if let Ok(mut guard) = state.config.lock() {
        guard.settings.autostart = enabled;
        let _ = guard.save(&state.path);
    }
    Ok(())
}

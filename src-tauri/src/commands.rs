use crate::config::{AppConfig, ProjectTemplate};
use crate::detect;
use crate::git;
use crate::launcher;
use crate::tray;
use crate::AppState;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

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
            hotkey: std::sync::Mutex::new(None),
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
            favorite: false,
            last_launched_at: None,
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

    #[test]
    fn detect_dir_errors_on_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope-xyz");
        assert!(detect_dir(missing.to_str().unwrap()).unwrap_err().contains("目录不存在"));
    }

    #[test]
    fn detect_dir_returns_node_suggestions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        let res = detect_dir(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(res.suggestions[0].command, "npm run dev");
    }

    #[test]
    fn normalize_for_compare_normalizes_windows_paths() {
        assert_eq!(normalize_for_compare("D:/Projects/App/"), "d:\\projects\\app");
        assert_eq!(normalize_for_compare("D:\\Projects\\App"), "d:\\projects\\app");
    }

    #[test]
    fn scan_with_config_marks_already_imported() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app/.git")).unwrap();
        let mut cfg = AppConfig::default();
        cfg.projects.push(Project {
            id: "p1".into(),
            name: "App".into(),
            root_dir: dir.path().join("app").to_string_lossy().to_uppercase(),
            favorite: false,
            last_launched_at: None,
            items: vec![],
        });
        let got = scan_with_config(dir.path().to_str().unwrap(), &cfg).unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[0].already_imported);
    }

    #[test]
    fn scan_with_config_errors_on_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope-xyz");
        let cfg = AppConfig::default();
        assert!(scan_with_config(missing.to_str().unwrap(), &cfg).unwrap_err().contains("目录不存在"));
    }

    #[test]
    fn touch_last_launched_updates_known_project() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(sample_project("p1"));
        assert!(touch_last_launched(&mut cfg, "p1", 123));
        assert_eq!(cfg.projects[0].last_launched_at, Some(123));
    }

    #[test]
    fn touch_last_launched_ignores_unknown_project() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(sample_project("p1"));
        assert!(!touch_last_launched(&mut cfg, "nope", 123));
        assert_eq!(cfg.projects[0].last_launched_at, None);
    }

    #[test]
    fn project_dir_rejects_unknown_or_empty_root() {
        let cfg = AppConfig::default();
        assert!(project_dir(&cfg, "missing").is_err());
        let mut cfg2 = AppConfig::default();
        let mut p = sample_project("p1");
        p.root_dir = "  ".into();
        cfg2.projects.push(p);
        assert!(project_dir(&cfg2, "p1").is_err());
    }

    #[test]
    fn project_dir_returns_root() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(sample_project("p1"));
        assert_eq!(project_dir(&cfg, "p1").unwrap(), PathBuf::from(r"D:\Projects\PVDS"));
    }

    #[test]
    fn detected_project_serializes_already_imported() {
        let d = DetectedProject {
            name: "App".into(),
            root_dir: r"D:\App".into(),
            already_imported: true,
            ecosystems: vec![],
            suggestions: vec![],
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"alreadyImported\":true"), "{json}");
        assert!(json.contains("\"rootDir\""), "{json}");
    }
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: AppConfig) -> Result<(), String> {
    let git_path = config.settings.git_path.clone();
    apply_config(&state, config)?;
    git::set_configured_git(git_path.as_deref());
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

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// 纯函数：命中项目则更新时间戳。
pub fn touch_last_launched(cfg: &mut AppConfig, project_id: &str, ts: u64) -> bool {
    match cfg.projects.iter_mut().find(|p| p.id == project_id) {
        Some(p) => {
            p.last_launched_at = Some(ts);
            true
        }
        None => false,
    }
}

/// 启动成功后 best-effort 记录；失败不影响启动。
pub(crate) fn record_launch(app: &AppHandle, project_id: &str) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let mut guard = state.config.lock().unwrap();
    if touch_last_launched(&mut guard, project_id, now_secs()) {
        if let Err(e) = guard.save(&state.path) {
            eprintln!("record launch failed: {e}");
        }
    }
}

#[tauri::command]
pub fn hide_palette(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("palette") {
        let _ = win.hide();
    }
    Ok(())
}

#[tauri::command]
pub fn set_hotkey(app: AppHandle, state: State<AppState>, hotkey: String) -> Result<(), String> {
    let spec = hotkey.trim().to_string();
    let new_shortcut = crate::hotkey::parse(&spec)?;
    let old = state.hotkey.lock().unwrap().clone();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Err(e) = gs.register(new_shortcut) {
        if let Some(old_spec) = old {
            let _ = crate::hotkey::register(&app, &old_spec);
        }
        return Err(format!("快捷键注册失败（可能已被其他程序占用）：{e}"));
    }
    let mut guard = state.config.lock().unwrap();
    let previous = guard.clone();
    guard.settings.hotkey = spec.clone();
    if let Err(e) = guard.save(&state.path) {
        *guard = previous;
        drop(guard);
        if let Some(old_spec) = old {
            let _ = crate::hotkey::register(&app, &old_spec);
        }
        return Err(e);
    }
    drop(guard);
    *state.hotkey.lock().unwrap() = Some(spec);
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
    let result = launcher::launch_project(&app, &cfg, &project_id);
    if result.is_ok() {
        record_launch(&app, &project_id);
    }
    result
}

#[tauri::command]
pub fn launch_item_cmd(app: AppHandle, state: State<AppState>, project_id: String, item_id: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    let result = launcher::launch_item(&app, &cfg, &project_id, &item_id);
    if result.is_ok() {
        record_launch(&app, &project_id);
    }
    result
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
    let old_hotkey = state.config.lock().unwrap().settings.hotkey.clone();
    let new_hotkey = cfg.settings.hotkey.clone();
    if new_hotkey != old_hotkey {
        if let Err(e) = crate::hotkey::register(&app, &new_hotkey) {
            let _ = crate::hotkey::register(&app, &old_hotkey);
            return Err(format!("导入失败：{e}"));
        }
    }
    backup_config_file(&state.path);
    let git_path = cfg.settings.git_path.clone();
    if let Err(e) = apply_config(&state, cfg) {
        if new_hotkey != old_hotkey {
            let _ = crate::hotkey::register(&app, &old_hotkey);
        }
        return Err(e);
    }
    git::set_configured_git(git_path.as_deref());
    if let Ok(mut g) = state.hotkey.lock() {
        *g = Some(new_hotkey);
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProject {
    pub name: String,
    pub root_dir: String,
    pub already_imported: bool,
    pub ecosystems: Vec<String>,
    pub suggestions: Vec<detect::Suggestion>,
}

#[tauri::command(async)]
pub fn detect_project(path: String) -> Result<detect::DetectResult, String> {
    detect_dir(&path)
}

pub fn detect_dir(path: &str) -> Result<detect::DetectResult, String> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("目录不存在：{path}"));
    }
    Ok(detect::detect(p))
}

#[tauri::command(async)]
pub fn scan_workspace(state: State<'_, AppState>, path: String) -> Result<Vec<DetectedProject>, String> {
    let cfg = state.config.lock().unwrap().clone();
    scan_with_config(&path, &cfg)
}

pub fn scan_with_config(path: &str, cfg: &AppConfig) -> Result<Vec<DetectedProject>, String> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("目录不存在：{path}"));
    }
    let existing: Vec<String> = cfg
        .projects
        .iter()
        .map(|project| normalize_for_compare(&project.root_dir))
        .collect();
    Ok(detect::scan(p)
        .into_iter()
        .map(|repo| DetectedProject {
            already_imported: existing
                .iter()
                .any(|e| e == &normalize_for_compare(&repo.root_dir)),
            name: repo.name,
            root_dir: repo.root_dir,
            ecosystems: repo.ecosystems,
            suggestions: repo.suggestions,
        })
        .collect())
}

pub fn project_dir(cfg: &AppConfig, project_id: &str) -> Result<PathBuf, String> {
    let p = cfg
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("项目不存在：{project_id}"))?;
    if p.root_dir.trim().is_empty() {
        return Err("项目未设置根目录".into());
    }
    Ok(PathBuf::from(&p.root_dir))
}

#[tauri::command(async)]
pub fn git_statuses(state: State<'_, AppState>, project_ids: Vec<String>) -> Vec<git::RepoStatus> {
    let cfg = state.config.lock().unwrap().clone();
    let targets: Vec<(String, Result<PathBuf, String>)> = project_ids
        .iter()
        .map(|id| (id.clone(), project_dir(&cfg, id)))
        .collect();

    let n = targets.len();
    let results: std::sync::Mutex<Vec<Option<git::RepoStatus>>> =
        std::sync::Mutex::new((0..n).map(|_| None).collect());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = 4usize.min(n.max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let (id, dir) = &targets[i];
                let st = match dir {
                    Ok(dir) => git::repo_status(id, dir),
                    Err(e) => git::RepoStatus::errored(id, e.clone()),
                };
                let mut guard = results.lock().unwrap();
                guard[i] = Some(st);
            });
        }
    });
    results.into_inner().unwrap().into_iter().flatten().collect()
}

#[tauri::command(async)]
pub fn git_log(
    state: State<'_, AppState>,
    project_id: String,
    limit: Option<u32>,
    skip: Option<u32>,
) -> Result<Vec<git::GraphRow>, String> {
    let cfg = state.config.lock().unwrap().clone();
    let dir = project_dir(&cfg, &project_id)?;
    let commits = git::git_log(&dir, limit.unwrap_or(100), skip.unwrap_or(0))?;
    Ok(git::assign_lanes(&commits))
}

#[tauri::command(async)]
pub fn git_commit_detail(
    state: State<'_, AppState>,
    project_id: String,
    hash: String,
) -> Result<git::CommitDetail, String> {
    let cfg = state.config.lock().unwrap().clone();
    let dir = project_dir(&cfg, &project_id)?;
    git::commit_detail(&dir, &hash)
}

#[tauri::command(async)]
pub fn git_branches(state: State<'_, AppState>, project_id: String) -> Result<Vec<git::BranchInfo>, String> {
    let cfg = state.config.lock().unwrap().clone();
    let dir = project_dir(&cfg, &project_id)?;
    git::branches(&dir)
}

#[tauri::command(async)]
pub fn git_file_diff(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
    staged: bool,
    ignore_whitespace: bool,
    full_context: bool,
) -> Result<git::FileDiff, String> {
    if path.trim().is_empty() {
        return Err("文件路径为空".into());
    }
    let cfg = state.config.lock().unwrap().clone();
    let dir = project_dir(&cfg, &project_id)?;
    git::file_diff(&dir, &path, staged, ignore_whitespace, full_context)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub configured: Option<String>,
    pub resolved: Option<String>,
}

/// 设置页用：当前配置的 git 路径与实际解析结果（None = 未找到）。
#[tauri::command]
pub fn get_git_info() -> GitInfo {
    GitInfo {
        configured: git::configured_git(),
        resolved: git::resolve_git_path().map(|p| p.display().to_string()),
    }
}

/// Windows 路径比较归一化：统一分隔符、去尾分隔符、不区分大小写。
pub fn normalize_for_compare(path: &str) -> String {
    path.trim_end_matches(|c| c == '\\' || c == '/')
        .replace('/', "\\")
        .to_ascii_lowercase()
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

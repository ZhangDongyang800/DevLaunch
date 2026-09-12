use serde::{Deserialize, Serialize};
use serde::de::Error as _;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONFIG_VERSION: u32 = 4;
pub const MODERN_VERSION: u32 = 3;
pub const TEMPLATE_VERSION: u32 = 3;
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+D";

fn default_hotkey() -> String {
    DEFAULT_HOTKEY.to_string()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    #[default]
    Cmd,
    PowerShell,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub work_dir: Option<String>,
    #[serde(default)]
    pub shell: Shell,
    pub command: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root_dir: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub last_launched_at: Option<u64>,
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub autostart: bool,
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { autostart: false, hotkey: default_hotkey() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub version: u32,
    pub settings: Settings,
    pub projects: Vec<Project>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { version: CONFIG_VERSION, settings: Settings::default(), projects: Vec::new() }
    }
}

pub struct LoadedConfig {
    pub config: AppConfig,
    pub corrupt_backup: Option<PathBuf>,
}

impl AppConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(path: &Path) -> AppConfig {
        Self::load_diagnostic(path).config
    }

    pub fn load_diagnostic(path: &Path) -> LoadedConfig {
        match fs::read_to_string(path) {
            Ok(text) => match parse_config(&text) {
                Ok(config) => LoadedConfig { config, corrupt_backup: None },
                Err(e) => {
                    eprintln!("config parse failed: {e}; backing up and using defaults");
                    let corrupt_backup = backup_corrupt(path).ok();
                    LoadedConfig { config: AppConfig::new(), corrupt_backup }
                }
            },
            Err(_) => LoadedConfig { config: AppConfig::new(), corrupt_backup: None },
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        save_json(self, path)
    }
}

/// 空 id / 重复 id 补发 UUID，保证 id 可作为稳定标识使用。
pub fn normalize_item_ids(items: &mut [Item]) {
    let mut seen = HashSet::new();
    for item in items {
        if item.id.trim().is_empty() || !seen.insert(item.id.clone()) {
            item.id = uuid::Uuid::new_v4().to_string();
            seen.insert(item.id.clone());
        }
    }
}

pub fn normalize_ids(cfg: &mut AppConfig) {
    let mut project_seen = HashSet::new();
    for project in &mut cfg.projects {
        if project.id.trim().is_empty() || !project_seen.insert(project.id.clone()) {
            project.id = uuid::Uuid::new_v4().to_string();
            project_seen.insert(project.id.clone());
        }
        normalize_item_ids(&mut project.items);
    }
}

pub fn save_json<T: Serialize>(value: &T, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn backup_corrupt(path: &Path) -> std::io::Result<PathBuf> {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let bak = path.with_extension(format!("json.corrupt-{ts}"));
    fs::rename(path, &bak)?;
    Ok(bak)
}

/// 版本探测后选择现代格式直接解析或 legacy 迁移（v1/v2）；无 version 但含 items 视为现代格式。
fn has_items_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key("items") || map.values().any(has_items_key)
        }
        serde_json::Value::Array(arr) => arr.iter().any(has_items_key),
        _ => false,
    }
}

fn probe_modern(value: &serde_json::Value) -> bool {
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(v) => v >= u64::from(MODERN_VERSION),
        None => has_items_key(value),
    }
}

fn has_key(value: &serde_json::Value, key: &str) -> bool {
    matches!(value, serde_json::Value::Object(map) if map.contains_key(key))
}

fn reject_too_new(value: &serde_json::Value, max_version: u32) -> Result<(), serde_json::Error> {
    if let Some(v) = value.get("version").and_then(serde_json::Value::as_u64) {
        if v > u64::from(max_version) {
            return Err(serde_json::Error::custom(format!(
                "文件版本 v{v} 高于当前 DevLaunch 支持的 v{max_version}，请升级应用后再导入"
            )));
        }
    }
    Ok(())
}

pub fn parse_config(text: &str) -> Result<AppConfig, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    reject_too_new(&value, CONFIG_VERSION)?;
    if !has_key(&value, "projects") {
        return Err(serde_json::Error::custom(
            "这不是 DevLaunch 配置备份（缺少 projects 字段）；请选择通过「设置 → 导出」生成的配置文件",
        ));
    }
    let mut cfg = if probe_modern(&value) {
        let mut cfg: AppConfig = serde_json::from_value(value)?;
        if cfg.version < CONFIG_VERSION {
            cfg.version = CONFIG_VERSION;
        }
        cfg
    } else {
        let legacy: LegacyAppConfig = serde_json::from_value(value)?;
        legacy.into_config()
    };
    normalize_ids(&mut cfg);
    Ok(cfg)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyAppConfig {
    #[serde(default)]
    settings: LegacySettings,
    #[serde(default)]
    projects: Vec<LegacyProject>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LegacySettings {
    autostart: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProject {
    id: String,
    name: String,
    root_dir: String,
    #[serde(default)]
    groups: Vec<LegacyGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyGroup {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    terminal: LegacyTerminal,
    #[serde(default)]
    steps: Vec<LegacyStep>,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum LegacyTerminal {
    #[default]
    Cmd,
    PowerShell,
    WindowsTerminal,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStep {
    #[serde(default)]
    work_dir: Option<String>,
    #[serde(default)]
    terminal: LegacyTerminal,
    #[serde(default)]
    command: String,
}

impl LegacyAppConfig {
    fn into_config(self) -> AppConfig {
        AppConfig {
            version: CONFIG_VERSION,
            settings: Settings { autostart: self.settings.autostart, ..Settings::default() },
            projects: self.projects.into_iter().map(LegacyProject::into_config).collect(),
        }
    }
}

impl LegacyProject {
    fn into_config(self) -> Project {
        Project {
            id: self.id,
            name: self.name,
            root_dir: self.root_dir,
            favorite: false,
            last_launched_at: None,
            items: self.groups.into_iter().filter_map(LegacyGroup::into_item).collect(),
        }
    }
}

impl LegacyGroup {
    fn shell(&self) -> Shell {
        let chosen = if self.terminal != LegacyTerminal::Cmd {
            self.terminal
        } else {
            self.steps
                .iter()
                .map(|s| s.terminal)
                .find(|t| *t != LegacyTerminal::Cmd)
                .unwrap_or(LegacyTerminal::Cmd)
        };
        match chosen {
            LegacyTerminal::PowerShell => Shell::PowerShell,
            _ => Shell::Cmd,
        }
    }

    /// 同一 group 的步骤合并为一个启动项；workDir 变化处插入 cd 行。
    fn into_item(self) -> Option<Item> {
        let shell = self.shell();
        let work_dir = self.steps.first().and_then(|s| s.work_dir.clone());
        let mut prev = work_dir.clone();
        let mut lines: Vec<String> = Vec::new();
        for step in &self.steps {
            let cmd = step.command.trim();
            if cmd.is_empty() {
                continue;
            }
            if step.work_dir != prev {
                let rel = rel_path(prev.as_deref(), step.work_dir.as_deref());
                if rel != "." {
                    lines.push(cd_line(shell, &rel));
                }
                prev = step.work_dir.clone();
            }
            lines.push(cmd.to_string());
        }
        if lines.is_empty() {
            return None;
        }
        Some(Item { id: self.id, name: self.name, work_dir, shell, command: lines.join("\n") })
    }
}

fn cd_line(shell: Shell, dir: &str) -> String {
    match shell {
        Shell::Cmd => format!("cd /d \"{dir}\""),
        Shell::PowerShell => format!("Set-Location -LiteralPath '{}'", dir.replace('\'', "''")),
    }
}

/// 从 from 目录到 to 目录的相对路径；to 为绝对路径时原样返回；None = 根目录。
fn rel_path(from: Option<&str>, to: Option<&str>) -> String {
    if let Some(t) = to {
        if Path::new(t).is_absolute() {
            return t.to_string();
        }
    }
    let split = |s: Option<&str>| -> Vec<String> {
        s.unwrap_or("")
            .split(['\\', '/'])
            .filter(|c| !c.is_empty() && *c != ".")
            .map(str::to_string)
            .collect()
    };
    let f = split(from);
    let t = split(to);
    let common = f.iter().zip(t.iter()).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = std::iter::repeat("..".to_string()).take(f.len() - common).collect();
    parts.extend(t[common..].iter().cloned());
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("\\")
    }
}

// ProjectTemplate v3
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTemplate {
    #[serde(default = "default_template_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
}

fn default_template_version() -> u32 {
    1
}

impl ProjectTemplate {
    pub fn from_project(p: &Project) -> Self {
        Self { version: TEMPLATE_VERSION, name: p.name.clone(), items: p.items.clone() }
    }

    pub fn load(path: &Path) -> Result<ProjectTemplate, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("读取失败：{e}"))?;
        parse_template(&text).map_err(|e| format!("配置文件格式错误：{e}"))
    }
}

pub fn parse_template(text: &str) -> Result<ProjectTemplate, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    reject_too_new(&value, TEMPLATE_VERSION)?;
    if has_key(&value, "projects") || !(has_key(&value, "items") || has_key(&value, "groups")) {
        return Err(serde_json::Error::custom(
            "这不是项目配置文件（需要 items 或 groups 字段）；请选择「导出到项目根」生成的文件",
        ));
    }
    let mut tpl = if probe_modern(&value) {
        let mut tpl: ProjectTemplate = serde_json::from_value(value)?;
        if tpl.version < TEMPLATE_VERSION {
            tpl.version = TEMPLATE_VERSION;
        }
        tpl
    } else {
        let legacy: LegacyTemplate = serde_json::from_value(value)?;
        legacy.into_template()
    };
    normalize_item_ids(&mut tpl.items);
    Ok(tpl)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyTemplate {
    #[serde(default)]
    name: String,
    #[serde(default)]
    groups: Vec<LegacyGroup>,
}

impl LegacyTemplate {
    fn into_template(self) -> ProjectTemplate {
        ProjectTemplate {
            version: TEMPLATE_VERSION,
            name: self.name,
            items: self.groups.into_iter().filter_map(LegacyGroup::into_item).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_project() -> Project {
        Project {
            id: "p1".into(),
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
    fn settings_default_uses_default_hotkey() {
        assert_eq!(Settings::default().hotkey, DEFAULT_HOTKEY);
        assert_eq!(DEFAULT_HOTKEY, "Ctrl+Alt+D");
    }

    #[test]
    fn v3_config_migrates_to_v4_preserving_projects_and_items() {
        let v3 = r#"{
            "version": 3,
            "settings": {"autostart": true},
            "projects": [{
                "id": "p1", "name": "XingTu", "rootDir": "D:\\proj",
                "items": [{"id": "i1", "name": "后端", "workDir": "backend", "shell": "cmd", "command": "python app.py"}]
            }]
        }"#;
        let cfg = parse_config(v3).unwrap();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert!(cfg.settings.autostart);
        assert_eq!(cfg.settings.hotkey, DEFAULT_HOTKEY);
        assert_eq!(cfg.projects.len(), 1);
        assert_eq!(cfg.projects[0].items.len(), 1);
        assert_eq!(cfg.projects[0].items[0].command, "python app.py");
        assert!(!cfg.projects[0].favorite);
        assert_eq!(cfg.projects[0].last_launched_at, None);
    }

    #[test]
    fn new_project_fields_serialize_camel_case() {
        let mut p = sample_project();
        p.favorite = true;
        p.last_launched_at = Some(1757577600);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"favorite\":true"), "{json}");
        assert!(json.contains("\"lastLaunchedAt\":1757577600"), "{json}");
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn template_keeps_template_version_three() {
        let tpl = ProjectTemplate::from_project(&sample_project());
        assert_eq!(tpl.version, TEMPLATE_VERSION);
        let json = serde_json::to_string(&tpl).unwrap();
        let back = parse_template(&json).unwrap();
        assert_eq!(back.version, TEMPLATE_VERSION);
    }

    #[test]
    fn too_new_rejected_per_file_type() {
        assert!(parse_config(r#"{"version":5,"projects":[]}"#).is_err());
        assert!(parse_config(r#"{"version":4,"projects":[]}"#).is_ok());
        assert!(parse_template(r#"{"version":4,"name":"X","items":[]}"#).is_err());
        assert!(parse_template(r#"{"version":3,"name":"X","items":[]}"#).is_ok());
    }

    #[test]
    fn shell_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Shell::Cmd).unwrap(), "\"cmd\"");
        assert_eq!(serde_json::to_string(&Shell::PowerShell).unwrap(), "\"powershell\"");
        assert_eq!(Shell::default(), Shell::Cmd);
    }

    #[test]
    fn serializes_camel_case() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(sample_project());
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"rootDir\""));
        assert!(json.contains("\"workDir\""));
        assert!(json.contains("\"shell\":\"cmd\""));
        assert!(json.contains("\"items\""));
        assert!(!json.contains("readyCondition"));
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn project_template_strips_root_dir() {
        let tpl = ProjectTemplate::from_project(&sample_project());
        let json = serde_json::to_string(&tpl).unwrap();
        assert!(!json.contains("rootDir"));
        assert!(json.contains("\"name\":\"PVDS\""));
        assert!(json.contains("\"items\""));
    }

    #[test]
    fn project_template_roundtrip() {
        let tpl = ProjectTemplate::from_project(&sample_project());
        let json = serde_json::to_string(&tpl).unwrap();
        let back: ProjectTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tpl);
    }

    #[test]
    fn v2_config_migrates_to_items() {
        let v2 = r#"{
            "version": 2,
            "settings": {"readyTimeoutSec": 30, "autostart": true},
            "projects": [{
                "id": "p1", "name": "XingTu", "rootDir": "D:\\Projects\\XINGTU",
                "groups": [
                    {"id": "g1", "name": "后端", "terminal": "cmd", "steps": [
                        {"id": "s1", "name": "", "workDir": "backend", "terminal": "cmd",
                         "command": "conda activate xingtu",
                         "readyCondition": {"type": "delay", "seconds": 5}},
                        {"id": "s2", "name": "", "workDir": "backend", "terminal": "cmd",
                         "command": "python -m uvicorn main:app",
                         "readyCondition": {"type": "port", "port": 8081, "host": "127.0.0.1", "timeoutSec": 30}}
                    ]},
                    {"id": "g2", "name": "前端", "terminal": "powershell", "steps": [
                        {"id": "s3", "name": "", "workDir": "frontend", "terminal": "powershell",
                         "command": "npm run dev", "readyCondition": {"type": "immediate"}}
                    ]}
                ]
            }]
        }"#;
        let cfg = parse_config(v2).unwrap();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert!(cfg.settings.autostart);
        let p = &cfg.projects[0];
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[0].name, "后端");
        assert_eq!(p.items[0].work_dir.as_deref(), Some("backend"));
        assert_eq!(p.items[0].shell, Shell::Cmd);
        assert_eq!(p.items[0].command, "conda activate xingtu\npython -m uvicorn main:app");
        assert_eq!(p.items[1].shell, Shell::PowerShell);
        assert_eq!(p.items[1].command, "npm run dev");
    }

    #[test]
    fn v2_multi_workdir_inserts_cd_lines() {
        let v2 = r#"{
            "version": 2,
            "projects": [{
                "id": "p1", "name": "X", "rootDir": "D:\\proj",
                "groups": [{"id": "g1", "name": "默认", "terminal": "cmd", "steps": [
                    {"id": "s1", "workDir": "backend", "terminal": "cmd", "command": "npm i"},
                    {"id": "s2", "workDir": "frontend", "terminal": "cmd", "command": "npm run dev"},
                    {"id": "s3", "workDir": null, "terminal": "cmd", "command": "echo done"}
                ]}]
            }]
        }"#;
        let cfg = parse_config(v2).unwrap();
        assert_eq!(
            cfg.projects[0].items[0].command,
            "npm i\ncd /d \"..\\frontend\"\nnpm run dev\ncd /d \"..\"\necho done"
        );
    }

    #[test]
    fn v1_config_infers_group_shell_from_steps() {
        let v1 = r#"{
            "version": 1,
            "projects": [{
                "id": "p1", "name": "X", "rootDir": "D:\\proj",
                "groups": [{"id": "g1", "name": "默认", "steps": [
                    {"id": "s1", "terminal": "cmd", "command": "a"},
                    {"id": "s2", "terminal": "powershell", "command": "b"}
                ]}]
            }]
        }"#;
        let cfg = parse_config(v1).unwrap();
        // v1 无组终端：取第一个非 cmd 步骤终端
        assert_eq!(cfg.projects[0].items[0].shell, Shell::PowerShell);
    }

    #[test]
    fn windowsterminal_maps_to_cmd() {
        let v2 = r#"{
            "version": 2,
            "projects": [{
                "id": "p1", "name": "X", "rootDir": "D:\\proj",
                "groups": [{"id": "g1", "name": "默认", "terminal": "windowsterminal", "steps": [
                    {"id": "s1", "terminal": "windowsterminal", "command": "a"}
                ]}]
            }]
        }"#;
        let cfg = parse_config(v2).unwrap();
        assert_eq!(cfg.projects[0].items[0].shell, Shell::Cmd);
    }

    #[test]
    fn legacy_template_migrates_and_exports_v3() {
        let tpl = r#"{"version":2,"name":"PVDS","groups":[{"id":"g1","name":"默认","terminal":"cmd","steps":[{"id":"s1","workDir":"server","terminal":"cmd","command":"python app.py","readyCondition":{"type":"immediate"}}]}]}"#;
        let t = parse_template(tpl).unwrap();
        assert_eq!(t.version, TEMPLATE_VERSION);
        assert_eq!(t.items.len(), 1);
        assert_eq!(t.items[0].work_dir.as_deref(), Some("server"));
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"items\""));
        assert!(!json.contains("groups"));
        assert!(!json.contains("readyCondition"));
    }

    #[test]
    fn versionless_v3_config_with_items_parses() {
        let json = r#"{
            "settings": {"autostart": true},
            "projects": [{
                "id": "p1", "name": "XingTu", "rootDir": "D:\\proj",
                "items": [{"id": "i1", "name": "后端", "workDir": "backend", "shell": "cmd", "command": "python app.py"}]
            }]
        }"#;
        let cfg = parse_config(json).unwrap();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.projects[0].items.len(), 1);
        assert_eq!(cfg.projects[0].items[0].work_dir.as_deref(), Some("backend"));
        assert_eq!(cfg.projects[0].items[0].command, "python app.py");
    }

    #[test]
    fn versionless_v3_template_with_items_parses() {
        let json = r#"{"name":"XingTu","items":[{"id":"i1","name":"后端","shell":"cmd","command":"python app.py"}]}"#;
        let tpl = parse_template(json).unwrap();
        assert_eq!(tpl.version, TEMPLATE_VERSION);
        assert_eq!(tpl.name, "XingTu");
        assert_eq!(tpl.items.len(), 1);
        assert_eq!(tpl.items[0].command, "python app.py");
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = AppConfig::load(&path);
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert!(!cfg.settings.autostart);
        assert!(cfg.projects.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = AppConfig::default();
        cfg.settings.autostart = true;
        cfg.save(&path).unwrap();
        let loaded = AppConfig::load(&path);
        assert!(loaded.settings.autostart);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn corrupt_file_backed_up_and_defaulted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not valid json").unwrap();
        let cfg = AppConfig::load(&path);
        assert!(cfg.projects.is_empty());
        let backed_up = fs::read_dir(dir.path()).unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("config.json.corrupt-"));
        assert!(backed_up);
    }

    #[test]
    fn readme_template_without_item_ids_parses_with_generated_ids() {
        let json = r#"{"version":3,"name":"MyApp","items":[{"name":"Backend","workDir":"backend","shell":"cmd","command":"python app.py"},{"name":"Frontend","shell":"cmd","command":"npm run dev"}]}"#;
        let tpl = parse_template(json).unwrap();
        assert_eq!(tpl.items.len(), 2);
        assert!(!tpl.items[0].id.trim().is_empty());
        assert!(!tpl.items[1].id.trim().is_empty());
        assert_ne!(tpl.items[0].id, tpl.items[1].id);
    }

    #[test]
    fn config_with_missing_item_ids_parses_and_normalizes() {
        let json = r#"{"version":3,"projects":[{"id":"p1","name":"X","rootDir":"D:\\p","items":[{"name":"a","shell":"cmd","command":"a"},{"name":"b","shell":"cmd","command":"b"}]}]}"#;
        let cfg = parse_config(json).unwrap();
        let items = &cfg.projects[0].items;
        assert!(!items[0].id.trim().is_empty());
        assert_ne!(items[0].id, items[1].id);
    }

    #[test]
    fn duplicate_ids_are_replaced() {
        let json = r#"{"version":3,"projects":[{"id":"p1","name":"X","rootDir":"D:\\p","items":[{"id":"same","name":"a","shell":"cmd","command":"a"},{"id":"same","name":"b","shell":"cmd","command":"b"}]},{"id":"p1","name":"Y","rootDir":"D:\\q","items":[]}]}"#;
        let cfg = parse_config(json).unwrap();
        assert_eq!(cfg.projects.len(), 2);
        assert_ne!(cfg.projects[0].id, cfg.projects[1].id);
        assert_ne!(cfg.projects[0].items[0].id, cfg.projects[0].items[1].id);
    }

    #[test]
    fn legacy_template_rejected_as_config() {
        let legacy_tpl = r#"{"version":2,"name":"PVDS","groups":[{"id":"g1","name":"默认","terminal":"cmd","steps":[{"id":"s1","terminal":"cmd","command":"a"}]}]}"#;
        let err = parse_config(legacy_tpl).unwrap_err().to_string();
        assert!(err.contains("projects"), "{err}");
    }

    #[test]
    fn project_template_rejected_as_config() {
        let tpl = r#"{"version":3,"name":"PVDS","items":[{"id":"i1","name":"a","shell":"cmd","command":"a"}]}"#;
        let err = parse_config(tpl).unwrap_err().to_string();
        assert!(err.contains("projects"), "{err}");
    }

    #[test]
    fn config_rejected_as_template() {
        let cfg = r#"{"version":3,"settings":{"autostart":false},"projects":[]}"#;
        let err = parse_template(cfg).unwrap_err().to_string();
        assert!(err.contains("items") || err.contains("groups"), "{err}");
    }

    #[test]
    fn load_diagnostic_reports_corrupt_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not valid json").unwrap();
        let loaded = AppConfig::load_diagnostic(&path);
        assert!(loaded.config.projects.is_empty());
        let backup = loaded.corrupt_backup.expect("corrupt backup path");
        assert!(backup.is_file());
    }

    #[test]
    fn legacy_group_with_empty_id_gets_normalized() {
        let v1 = r#"{"version":1,"projects":[{"id":"p1","name":"X","rootDir":"D:\\proj","groups":[{"terminal":"cmd","steps":[{"terminal":"cmd","command":"a"}]}]}]}"#;
        let cfg = parse_config(v1).unwrap();
        assert!(!cfg.projects[0].items[0].id.trim().is_empty());
    }
}

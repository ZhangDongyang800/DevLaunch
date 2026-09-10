use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONFIG_VERSION: u32 = 2;

fn default_host() -> String {
    "127.0.0.1".into()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Terminal {
    #[default]
    Cmd,
    PowerShell,
    WindowsTerminal,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReadyCondition {
    #[default]
    Immediate,
    Delay { seconds: u64 },
    Port { port: u16, #[serde(default = "default_host")] host: String, timeout_sec: u64 },
    Process { process_name: String, timeout_sec: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub ready_timeout_sec: u64,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { ready_timeout_sec: 30, autostart: false }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub work_dir: Option<String>,
    #[serde(default)]
    pub terminal: Terminal,
    pub command: String,
    #[serde(default)]
    pub ready_condition: ReadyCondition,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    /// v2 起终端是分组属性：一组共享一个终端窗口（组内步骤在同一 shell 里顺序执行）。
    #[serde(default)]
    pub terminal: Terminal,
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root_dir: String,
    pub groups: Vec<Group>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTemplate {
    #[serde(default = "default_template_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub groups: Vec<Group>,
}

fn default_template_version() -> u32 {
    1
}

impl ProjectTemplate {
    pub fn from_project(p: &Project) -> Self {
        Self { version: CONFIG_VERSION, name: p.name.clone(), groups: p.groups.clone() }
    }

    pub fn load(path: &Path) -> Result<ProjectTemplate, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("读取失败：{e}"))?;
        let mut tpl: ProjectTemplate = serde_json::from_str(&text).map_err(|e| format!("配置文件格式错误：{e}"))?;
        if tpl.version < CONFIG_VERSION {
            for group in tpl.groups.iter_mut() {
                migrate_group_terminal(group);
            }
            tpl.version = CONFIG_VERSION;
        }
        Ok(tpl)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub projects: Vec<Project>,
}

impl AppConfig {
    pub fn new() -> Self {
        Self { version: CONFIG_VERSION, ..Default::default() }
    }

    /// 反序列化后的版本归一：旧版本原位迁移到当前版本。
    pub fn migrate_if_needed(mut cfg: AppConfig) -> AppConfig {
        if cfg.version < CONFIG_VERSION {
            migrate_v1(&mut cfg);
        }
        cfg
    }

    pub fn load(path: &Path) -> AppConfig {
        match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(cfg) => Self::migrate_if_needed(cfg),
                Err(e) => {
                    eprintln!("config parse failed: {e}; backing up and using defaults");
                    let _ = backup_corrupt(path);
                    AppConfig::new()
                }
            },
            Err(_) => AppConfig::new(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        save_json(self, path)
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

fn backup_corrupt(path: &Path) -> std::io::Result<()> {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let bak = path.with_extension(format!("json.corrupt-{ts}"));
    fs::rename(path, bak)
}

/// v1→v2：终端从步骤级提升为分组级。取组内第一个非 cmd 终端作为组终端（全 cmd 则保持 cmd）。
fn migrate_v1(cfg: &mut AppConfig) {
    for group in cfg.projects.iter_mut().flat_map(|p| p.groups.iter_mut()) {
        migrate_group_terminal(group);
    }
    cfg.version = CONFIG_VERSION;
}

pub fn migrate_group_terminal(group: &mut Group) {
    if let Some(t) = group.steps.iter().map(|s| s.terminal).find(|t| *t != Terminal::Cmd) {
        group.terminal = t;
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
                    ready_condition: ReadyCondition::Port {
                        port: 8000,
                        host: "127.0.0.1".into(),
                        timeout_sec: 30,
                    },
                }],
            }],
        }
    }

    #[test]
    fn serializes_camel_case() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(sample_project());
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"readyTimeoutSec\":30"));
        assert!(json.contains("\"rootDir\""));
        assert!(json.contains("\"readyCondition\":{\"type\":\"port\""));
        assert!(json.contains("\"timeoutSec\":30"));
        assert!(json.contains("\"terminal\":\"cmd\""));
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.projects[0].groups[0].steps[0], cfg.projects[0].groups[0].steps[0]);
    }

    #[test]
    fn project_template_strips_root_dir() {
        let tpl = ProjectTemplate::from_project(&sample_project());
        let json = serde_json::to_string(&tpl).unwrap();
        assert!(!json.contains("rootDir"));
        assert!(json.contains("\"name\":\"PVDS\""));
        assert!(json.contains("\"groups\""));
    }

    #[test]
    fn project_template_roundtrip() {
        let tpl = ProjectTemplate::from_project(&sample_project());
        let json = serde_json::to_string(&tpl).unwrap();
        let back: ProjectTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tpl);
    }

    #[test]
    fn v1_config_migrates_group_terminal_from_steps() {
        let v1 = r#"{
            "version": 1,
            "settings": {"readyTimeoutSec": 30, "autostart": false},
            "projects": [{
                "id": "p1",
                "name": "PVDS",
                "rootDir": "D:\\Projects\\PVDS",
                "groups": [{
                    "id": "g1",
                    "name": "默认",
                    "steps": [
                        {"id": "s1", "name": "a", "terminal": "cmd", "command": "a", "readyCondition": {"type": "immediate"}},
                        {"id": "s2", "name": "b", "terminal": "powershell", "command": "b", "readyCondition": {"type": "immediate"}}
                    ]
                }]
            }]
        }"#;
        let cfg: AppConfig = serde_json::from_str(v1).unwrap();
        assert_eq!(cfg.version, 1); // 反序列化本身不迁移
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        cfg.save(&path).unwrap();
        let loaded = AppConfig::load(&path);
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.projects[0].groups[0].terminal, Terminal::PowerShell);
    }

    #[test]
    fn v2_group_terminal_survives_roundtrip() {
        let mut cfg = AppConfig::default();
        let mut project = sample_project();
        project.groups[0].terminal = Terminal::WindowsTerminal;
        cfg.projects.push(project);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        cfg.save(&path).unwrap();
        let loaded = AppConfig::load(&path);
        assert_eq!(loaded.projects[0].groups[0].terminal, Terminal::WindowsTerminal);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = AppConfig::load(&path);
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.settings.ready_timeout_sec, 30);
        assert!(cfg.projects.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = AppConfig::default();
        cfg.settings.ready_timeout_sec = 45;
        cfg.save(&path).unwrap();
        let loaded = AppConfig::load(&path);
        assert_eq!(loaded.settings.ready_timeout_sec, 45);
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
}

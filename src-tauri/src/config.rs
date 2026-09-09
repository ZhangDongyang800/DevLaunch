use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONFIG_VERSION: u32 = 1;

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

    pub fn load(path: &Path) -> AppConfig {
        match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(cfg) => cfg,
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
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

fn backup_corrupt(path: &Path) -> std::io::Result<()> {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let bak = path.with_extension(format!("json.corrupt-{ts}"));
    fs::rename(path, bak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn serializes_camel_case() {
        let mut cfg = AppConfig::default();
        cfg.projects.push(Project {
            id: "p1".into(),
            name: "PVDS".into(),
            root_dir: r"D:\Projects\PVDS".into(),
            groups: vec![Group {
                id: "g1".into(),
                name: "默认".into(),
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
        });
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
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = AppConfig::load(&path);
        assert_eq!(cfg.version, 1);
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

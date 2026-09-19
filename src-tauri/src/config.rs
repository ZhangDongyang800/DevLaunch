use serde::{Deserialize, Serialize};
use serde::de::Error as _;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONFIG_VERSION: u32 = 8;
pub const MODERN_VERSION: u32 = 3;
pub const TEMPLATE_VERSION: u32 = 4;
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+D";
pub const DEFAULT_THEME: &str = "signal";

/// 可选主题（= style.css 里的 [data-theme] 覆盖块）；只改配色不改布局。
pub const THEMES: [&str; 4] = ["signal", "graphite", "indigo", "amber"];

fn default_hotkey() -> String {
    DEFAULT_HOTKEY.to_string()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    #[default]
    Cmd,
    PowerShell,
    Bash,
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

/// 端口租约：git 知道 branch↔path，但端口必须跨创建/删除保持稳定，只能由配置持有。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeLease {
    pub branch: String,
    pub port: u16,
}

/// 按任务开发环境（git worktree）政策。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WorktreeSettings {
    /// worktree 根目录；相对 rootDir 解析，缺失时用 `<repo 同级>/<目录名>-wt`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// allow-list：新建环境时从主工作区复制的相对路径；默认空 = 不复制任何文件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copy: Vec<String>,
    /// 端口段起点；None = 不注入端口环境变量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_base: Option<u16>,
    /// 端口环境变量名，默认 `PORT`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leases: Vec<WorktreeLease>,
}

impl WorktreeSettings {
    pub const DEFAULT_PORT_KEY: &'static str = "PORT";
    pub const DEFAULT_ROOT_SUFFIX: &'static str = "-wt";

    pub fn effective_port_key(&self) -> &str {
        match self.port_key.as_deref().map(str::trim) {
            Some(k) if !k.is_empty() => k,
            _ => Self::DEFAULT_PORT_KEY,
        }
    }

    fn has_policy(&self) -> bool {
        self.root.is_some() || !self.copy.is_empty() || self.port_base.is_some() || self.port_key.is_some()
    }

    /// 模板视图：保留政策、丢弃租约（租约是本机状态，不进 devlaunch.json）。
    pub fn for_template(&self) -> Option<WorktreeSettings> {
        self.has_policy().then(|| WorktreeSettings { leases: Vec::new(), ..self.clone() })
    }
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
    /// 缺失 = 该仓库未启用按任务环境。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeSettings>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub autostart: bool,
    pub hotkey: String,
    /// 用户指定的 git.exe 绝对路径；空 = 自动检测（PATH / Program Files）。
    #[serde(default)]
    pub git_path: Option<String>,
    /// 配色主题名（前端 style.css 的 [data-theme]）；本机偏好，不进模板。
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}

/// 未知 / 空主题名归一到默认值，避免前端拿到没实现的主题名。
pub fn normalize_theme(name: &str) -> String {
    let trimmed = name.trim();
    if THEMES.contains(&trimmed) {
        trimmed.to_string()
    } else {
        DEFAULT_THEME.to_string()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autostart: false,
            hotkey: default_hotkey(),
            git_path: None,
            theme: default_theme(),
        }
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
    /// 磁盘内容损坏时，原件被备份到的路径。
    pub corrupt_backup: Option<PathBuf>,
    /// 磁盘内容损坏时，成功从哪个备份恢复；None = 没有可用备份（回退默认值）。
    pub restored_from: Option<PathBuf>,
    /// 文件存在但读不出来（被别的进程独占、IO 错误）。此时内存里是默认配置，
    /// 落盘会覆盖用户真实配置，所以整个会话进入写保护。
    pub blocked: Option<String>,
}

/// 读取配置文件的结果：三态必须区分开，读失败绝不能当成「没有配置」。
#[derive(Debug, PartialEq, Eq)]
enum ReadOutcome {
    Absent,
    Failed(String),
    Text(String),
}

/// 一次性的瞬时失败重试：Windows 上杀软/索引器短暂占用很常见。
fn read_config_file(path: &Path) -> ReadOutcome {
    let mut last = String::new();
    for attempt in 0..3 {
        match fs::read_to_string(path) {
            Ok(text) => return ReadOutcome::Text(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 目录项在也可能报 NotFound（例如目标是目录），所以再确认一次存在性。
                return if fs::symlink_metadata(path).is_ok() {
                    ReadOutcome::Failed(format!("{e}"))
                } else {
                    ReadOutcome::Absent
                };
            }
            Err(e) => {
                last = format!("{e}");
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(120));
                }
            }
        }
    }
    ReadOutcome::Failed(last)
}

/// 损坏时的最后手段：用最近的 `config.json.bak*` 备份（保存时滚动写入）。
fn newest_readable_backup(path: &Path) -> Option<(AppConfig, PathBuf)> {
    let dir = path.parent()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let Ok(entries) = fs::read_dir(dir) else { return None };
    let mut candidates: Vec<(std::time::SystemTime, PathBuf, AppConfig)> = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name == name || !file_name.starts_with(&name) || file_name.ends_with(".tmp") {
            continue;
        }
        let p = entry.path();
        let Ok(m) = entry.metadata() else { continue };
        let Ok(mtime) = m.modified() else { continue };
        if let ReadOutcome::Text(text) = read_config_file(&p) {
            if let Ok(cfg) = parse_config(&text) {
                candidates.push((mtime, p, cfg));
            }
        }
    }
    candidates.sort_by_key(|(t, _, _)| *t);
    candidates.pop().map(|(_, p, cfg)| (cfg, p))
}

impl AppConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(path: &Path) -> AppConfig {
        Self::load_diagnostic(path).config
    }

    pub fn load_diagnostic(path: &Path) -> LoadedConfig {
        let fallback = LoadedConfig {
            config: AppConfig::new(),
            corrupt_backup: None,
            restored_from: None,
            blocked: None,
        };
        match read_config_file(path) {
            ReadOutcome::Absent => fallback,
            ReadOutcome::Failed(e) => {
                eprintln!("config read failed: {e}; refusing to write over it");
                LoadedConfig { blocked: Some(e), ..fallback }
            }
            ReadOutcome::Text(text) => match parse_config(&text) {
                Ok(config) => LoadedConfig { config, ..fallback },
                Err(e) => {
                    eprintln!("config parse failed: {e}; backing up and recovering");
                    let corrupt_backup = backup_corrupt(path).ok();
                    match newest_readable_backup(path) {
                        Some((config, from)) => LoadedConfig {
                            config,
                            restored_from: Some(from),
                            corrupt_backup,
                            blocked: None,
                        },
                        None => LoadedConfig { corrupt_backup, ..fallback },
                    }
                }
            },
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        // 原子替换之前先留一份滚动备份：损坏/误写时才有东西可恢复。
        if path.is_file() {
            let _ = fs::copy(path, backup_path_of(path));
        }
        save_json(self, path)
    }
}

pub fn backup_path_of(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{name}.bak"))
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
    cfg.settings.theme = normalize_theme(&cfg.settings.theme);
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
            worktree: None,
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
        Shell::Bash => format!("cd '{}'", dir.replace('\\', "/").replace('\'', "'\\''")),
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

// ProjectTemplate v4
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTemplate {
    #[serde(default = "default_template_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
    /// 环境政策（root/copy/端口段）；租约不进模板，导出时已被丢弃。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeSettings>,
}

fn default_template_version() -> u32 {
    1
}

impl ProjectTemplate {
    pub fn from_project(p: &Project) -> Self {
        Self {
            version: TEMPLATE_VERSION,
            name: p.name.clone(),
            items: p.items.clone(),
            worktree: p.worktree.as_ref().and_then(WorktreeSettings::for_template),
        }
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
            worktree: None,
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
            worktree: None,
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
    fn template_keeps_template_version_four() {
        let tpl = ProjectTemplate::from_project(&sample_project());
        assert_eq!(tpl.version, TEMPLATE_VERSION);
        let json = serde_json::to_string(&tpl).unwrap();
        let back = parse_template(&json).unwrap();
        assert_eq!(back.version, TEMPLATE_VERSION);
    }

    #[test]
    fn too_new_rejected_per_file_type() {
        assert!(parse_config(r#"{"version":9,"projects":[]}"#).is_err());
        assert!(parse_config(r#"{"version":8,"projects":[]}"#).is_ok());
        assert!(parse_template(r#"{"version":5,"name":"X","items":[]}"#).is_err());
        assert!(parse_template(r#"{"version":4,"name":"X","items":[]}"#).is_ok());
    }

    #[test]
    fn shell_serializes_bash() {
        assert_eq!(serde_json::to_string(&Shell::Bash).unwrap(), "\"bash\"");
    }

    #[test]
    fn v4_config_migrates_to_v5_preserving_items() {
        let v4 = r#"{"version":4,"settings":{"autostart":false,"hotkey":"Ctrl+Alt+D"},
            "projects":[{"id":"p1","name":"X","rootDir":"D:\\p","favorite":true,"lastLaunchedAt":123,
            "items":[{"id":"i1","name":"bash项","shell":"bash","command":"npm run dev"}]}]}"#;
        let cfg = parse_config(v4).unwrap();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(CONFIG_VERSION, 8);
        assert!(cfg.projects[0].favorite);
        assert_eq!(cfg.projects[0].items[0].shell, Shell::Bash);
        assert_eq!(cfg.projects[0].items[0].command, "npm run dev");
    }

    #[test]
    fn settings_git_path_defaults_none_and_serializes_camel_case() {
        assert_eq!(Settings::default().git_path, None);
        let mut cfg = AppConfig::default();
        cfg.settings.git_path = Some(r"C:\Program Files\Git\cmd\git.exe".into());
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"gitPath\""), "{json}");
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.settings.git_path.as_deref(), Some(r"C:\Program Files\Git\cmd\git.exe"));
    }

    #[test]
    fn v5_config_migrates_to_v6_preserving_items() {
        let v5 = r#"{"version":5,"settings":{"autostart":false,"hotkey":"Ctrl+Alt+D"},
            "projects":[{"id":"p1","name":"X","rootDir":"D:\\p","favorite":true,
            "items":[{"id":"i1","name":"bash项","shell":"bash","command":"npm run dev"}]}]}"#;
        let cfg = parse_config(v5).unwrap();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.settings.git_path, None);
        assert!(cfg.projects[0].favorite);
        assert_eq!(cfg.projects[0].items[0].shell, Shell::Bash);
    }

    #[test]
    fn v6_config_migrates_to_v7_without_worktree() {
        let v6 = r#"{"version":6,"settings":{"autostart":false,"hotkey":"Ctrl+Alt+D","gitPath":"C:\\git.exe"},
            "projects":[{"id":"p1","name":"X","rootDir":"D:\\p","favorite":true,
            "items":[{"id":"i1","name":"bash项","shell":"bash","command":"npm run dev"}]}]}"#;
        let cfg = parse_config(v6).unwrap();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.settings.git_path.as_deref(), Some(r"C:\git.exe"));
        assert_eq!(cfg.projects[0].worktree, None);
        assert_eq!(cfg.projects[0].items[0].shell, Shell::Bash);
    }

    #[test]
    fn v7_config_migrates_to_v8_with_default_theme() {
        let v7 = r#"{"version":7,"settings":{"autostart":false,"hotkey":"Ctrl+Alt+D","gitPath":"C:\\git.exe"},
            "projects":[{"id":"p1","name":"X","rootDir":"D:\\p","favorite":true,
            "items":[{"id":"i1","name":"bash项","shell":"bash","command":"npm run dev"}]}]}"#;
        let cfg = parse_config(v7).unwrap();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.settings.theme, "signal");
        assert_eq!(cfg.settings.git_path.as_deref(), Some(r"C:\git.exe"));
        assert!(cfg.projects[0].favorite);
    }

    #[test]
    fn theme_round_trips_and_rejects_unknown_names() {
        let mut cfg = AppConfig::default();
        assert_eq!(Settings::default().theme, "signal");
        cfg.settings.theme = "indigo".into();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"theme\":\"indigo\""), "{json}");
        assert_eq!(parse_config(&json).unwrap().settings.theme, "indigo");

        // 手改 / 旧版本没写过的主题名一律归一到默认值，前端不会拿到未实现的主题
        assert_eq!(normalize_theme("  graphite  "), "graphite");
        assert_eq!(normalize_theme("light"), "signal");
        assert_eq!(normalize_theme(""), "signal");
        let weird = r#"{"version":8,"settings":{"autostart":false,"hotkey":"Ctrl+Alt+D","theme":"neon"},
            "projects":[]}"#;
        assert_eq!(parse_config(weird).unwrap().settings.theme, "signal");
    }

    #[test]
    fn worktree_settings_roundtrip_and_omit_empty_keys() {
        let mut p = sample_project();
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("worktree"), "{json}");

        p.worktree = Some(WorktreeSettings {
            root: Some(".wt".into()),
            copy: vec![".env".into()],
            port_base: Some(5173),
            port_key: None,
            leases: vec![WorktreeLease { branch: "feature/x".into(), port: 5173 }],
        });
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"root\":\".wt\""), "{json}");
        assert!(json.contains("\"portBase\":5173"), "{json}");
        assert!(json.contains("\"branch\":\"feature/x\""), "{json}");
        assert!(!json.contains("portKey"), "{json}");
        assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), p);
        assert_eq!(p.worktree.as_ref().unwrap().effective_port_key(), "PORT");
    }

    #[test]
    fn template_carries_worktree_policy_but_strips_leases() {
        let mut p = sample_project();
        p.worktree = Some(WorktreeSettings {
            root: None,
            copy: vec![".env".into()],
            port_base: Some(3000),
            port_key: Some("WEB_PORT".into()),
            leases: vec![WorktreeLease { branch: "a".into(), port: 3000 }],
        });
        let tpl = ProjectTemplate::from_project(&p);
        let json = serde_json::to_string(&tpl).unwrap();
        assert!(json.contains("\"copy\":[\".env\"]"), "{json}");
        assert!(json.contains("\"portKey\":\"WEB_PORT\""), "{json}");
        assert!(!json.contains("leases"), "{json}");
        assert_eq!(tpl.worktree.as_ref().unwrap().leases, Vec::new());

        // 只有租约、没有政策时不写 worktree 段
        let mut lease_only = sample_project();
        lease_only.worktree = Some(WorktreeSettings {
            leases: vec![WorktreeLease { branch: "a".into(), port: 3000 }],
            ..Default::default()
        });
        assert!(ProjectTemplate::from_project(&lease_only).worktree.is_none());
        assert!(!serde_json::to_string(&ProjectTemplate::from_project(&lease_only)).unwrap().contains("worktree"));
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
        assert!(loaded.blocked.is_none());
        assert!(loaded.restored_from.is_none());
    }

    /// 三态之一：文件不存在 = 首次运行，正常用默认配置，且不进写保护。
    #[test]
    fn missing_config_is_not_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let loaded = AppConfig::load_diagnostic(&path);
        assert!(loaded.blocked.is_none(), "{:?}", loaded.blocked);
        assert!(loaded.corrupt_backup.is_none());
        assert!(loaded.config.projects.is_empty());
    }

    /// 三态之二：文件存在但读不出来，绝不能当成「没有配置」。
    #[test]
    fn unreadable_config_is_reported_not_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        // 用同名目录制造「存在但读不出来」：杀软/占用进程在 Windows 上就是这个效果。
        let path = dir.path().join("config.json");
        fs::create_dir(&path).unwrap();
        let loaded = AppConfig::load_diagnostic(&path);
        assert!(loaded.blocked.is_some(), "read failure must surface as blocked");
        assert!(loaded.corrupt_backup.is_none());
        // 目录还在，没被当成可覆盖的空配置
        assert!(path.is_dir());
    }

    /// 三态之三：内容损坏时优先从备份恢复，而不是回退默认值。
    #[test]
    fn corrupt_config_restores_from_newest_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let older = dir.path().join("config.json.bak");
        let good = r#"{"version":7,"settings":{"autostart":false,"hotkey":"Ctrl+Alt+D"},"projects":[{"id":"p1","name":"救回来","rootDir":"D:\\x","items":[]}]}"#;
        fs::write(&older, good).unwrap();
        fs::write(&path, "{ not valid json").unwrap();

        let loaded = AppConfig::load_diagnostic(&path);
        assert_eq!(loaded.config.projects.len(), 1);
        assert_eq!(loaded.config.projects[0].name, "救回来");
        assert_eq!(loaded.restored_from.as_deref(), Some(older.as_path()));
        assert!(loaded.corrupt_backup.is_some());
        assert_eq!(loaded.blocked, None);
    }

    /// 损坏且没有任何可用备份时才回退默认值（并保持 corrupt_backup 上报）。
    #[test]
    fn corrupt_config_without_backup_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(dir.path().join("config.json.bak"), "{ broken too").unwrap();
        fs::write(&path, "{ not valid json").unwrap();
        let loaded = AppConfig::load_diagnostic(&path);
        assert!(loaded.restored_from.is_none());
        assert!(loaded.config.projects.is_empty());
        assert!(loaded.corrupt_backup.is_some());
    }

    /// 每次保存前留一份滚动备份，损坏/误写才有东西可恢复。
    #[test]
    fn save_keeps_previous_version_as_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut first = AppConfig::default();
        first.projects.push(Project {
            id: "p1".into(),
            name: "第一版".into(),
            root_dir: "D:\\x".into(),
            favorite: false,
            last_launched_at: None,
            items: vec![],
            worktree: None,
        });
        first.save(&path).unwrap();
        assert!(!backup_path_of(&path).exists(), "首次保存没有旧版本可备份");
        let mut second = first.clone();
        second.projects[0].name = "第二版".into();
        second.save(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap().matches("第二版").count(), 1);
        let bak = backup_path_of(&path);
        assert!(fs::read_to_string(&bak).unwrap().contains("第一版"));
    }

    #[test]
    fn read_outcome_classifies_absent_and_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        assert_eq!(read_config_file(&path), ReadOutcome::Absent);
        fs::write(&path, "hi").unwrap();
        assert_eq!(read_config_file(&path), ReadOutcome::Text("hi".into()));
    }

    #[test]
    fn legacy_group_with_empty_id_gets_normalized() {
        let v1 = r#"{"version":1,"projects":[{"id":"p1","name":"X","rootDir":"D:\\proj","groups":[{"terminal":"cmd","steps":[{"terminal":"cmd","command":"a"}]}]}]}"#;
        let cfg = parse_config(v1).unwrap();
        assert!(!cfg.projects[0].items[0].id.trim().is_empty());
    }
}

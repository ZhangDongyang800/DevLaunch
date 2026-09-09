# DevLaunch 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个 Windows 托盘常驻的 Tauri 2 应用：一次配置项目/分组/步骤，之后点击按钮即可自动打开真实终端、进入目录并按就绪条件顺序执行命令。

**Architecture:** Rust 核心负责配置（单写者 + 原子落盘）、终端平台抽象、就绪轮询与启动编排；Vue 3 前端是纯编辑器，所有变更经 IPC 由 Rust 写盘；托盘菜单由配置构建，配置变更后重建。

**Tech Stack:** Tauri 2、Rust（serde/serde_json/uuid/sysinfo + tauri-plugin-notification/autostart/dialog）、Vue 3 + TypeScript + Vite。

**Spec:** `docs/superpowers/specs/2026-09-09-devlaunch-design.md`（本计划依 spec 而写，执行者需同时阅读两者）

## Global Constraints

- Windows 优先；非 Windows 平台 `platform::spawn` 返回 `Unsupported` 错误（cfg 门控）
- 应用名 `DevLaunch`，identifier `com.devlaunch.app`
- 配置文件：`%APPDATA%\com.devlaunch.app\config.json`（Tauri `app_data_dir()`），原子写（temp + rename）
- JSON 字段 camelCase；`terminal` 取值 `"cmd" | "powershell" | "windowsterminal"`；默认终端 `cmd`
- `readyCondition` 为 serde tagged enum：`immediate | delay | port | process`；不含输出匹配
- 启动失败/就绪超时不吞错误：错误保留在终端窗口 + 系统通知 + 中断后续启动
- 管理窗口关闭 = 隐藏，不退出；退出只走托盘菜单
- 不做：输出匹配、依赖拓扑、进程监控/重试、内嵌终端、macOS/Linux 实现、自动更新
- 每个任务结束时 `cargo test`（src-tauri 内）与 `npm run build`（根目录）必须通过

---

### Task 1: 项目脚手架

**Files:**
- Create: 由 `create-tauri-app` 生成并迁移到仓库根目录
- Modify: `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`

**Interfaces:**
- Produces: 可编译运行的 Tauri 2 + Vue3 + TS 空应用；后续任务在 `src-tauri/src/` 与 `src/` 上扩展

- [ ] **Step 1: 生成脚手架**

在临时目录生成再迁回根目录（根目录已有 .git 与 docs，CTA 不接受非空目录）：

```bash
cd /d "%TEMP%\opencode"
npm create tauri-app@latest devlaunch -- --template vue-ts --manager npm --yes
robocopy devlaunch "D:\Projects\新建文件夹" /E /MOVE
```

- [ ] **Step 2: 安装依赖**

```bash
npm install
```

- [ ] **Step 3: 修改标识与窗口配置**

`src-tauri/tauri.conf.json`：`productName` 改为 `DevLaunch`，`identifier` 改为 `com.devlaunch.app`，window 节点改为：

```json
"windows": [
  {
    "title": "DevLaunch",
    "width": 960,
    "height": 680,
    "visible": false
  }
]
```

`src-tauri/Cargo.toml` 的 tauri 依赖增加 tray 特性并添加本计划所需插件依赖：

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-notification = "2"
tauri-plugin-dialog = "2"
tauri-plugin-autostart = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
sysinfo = "0.33"

[dev-dependencies]
tempfile = "3"
```

`package.json` 增加前端插件依赖：

```bash
npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-notification
```

- [ ] **Step 4: 验证编译与测试基建**

```bash
cargo test
npm run build
```

Expected: cargo test 通过（模板默认无测试则 0 tests ok）；vue-tsc + vite build 通过。

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: scaffold Tauri 2 + Vue 3 TS project (DevLaunch)"
```

---

### Task 2: 配置模块 config.rs

**Files:**
- Create: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/lib.rs`（声明 `pub mod config;`）

**Interfaces:**
- Produces: `AppConfig { version: u32, settings: Settings, projects: Vec<Project> }`；`Settings { ready_timeout_sec: u64, autostart: bool }`；`Project { id, name, root_dir, groups }`；`Group { id, name, steps }`；`Step { id, name, work_dir: Option<String>, terminal: Terminal, command, ready_condition: ReadyCondition }`；`Terminal` 枚举（Cmd/PowerShell/WindowsTerminal，serde 小写）；`ReadyCondition` tagged enum（Immediate/Delay{seconds}/Port{port,host,timeout_sec}/Process{process_name,timeout_sec}）；`AppConfig::load(&Path) -> AppConfig`（缺失/损坏时回退默认并备份）；`AppConfig::save(&self, &Path) -> Result<(), String>`（原子写）。JSON 均为 camelCase。

- [ ] **Step 1: 写失败测试**

`src-tauri/src/config.rs`：

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test
```

Expected: 编译失败（`config` 模块不存在）。

- [ ] **Step 3: 实现 config.rs**

```rust
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReadyCondition {
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
```

在 `src-tauri/src/lib.rs` 顶部加 `pub mod config;`（若模板为 `mod` 私有声明，改为 `pub mod`）。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test
```

Expected: 4 个测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: config types with atomic load/save"
```

---

### Task 3: 终端平台抽象 platform

**Files:**
- Create: `src-tauri/src/platform/mod.rs`
- Create: `src-tauri/src/platform/windows.rs`
- Modify: `src-tauri/src/lib.rs`（`pub mod platform;`）

**Interfaces:**
- Consumes: `config::Terminal`
- Produces: `platform::spawn(terminal: Terminal, work_dir: &str, command: &str) -> std::io::Result<()>`（非 Windows 返回 Unsupported）。测试用的纯字符串构建函数：`windows::cmd_command_line(work_dir, command) -> String`、`windows::powershell_command_line(...) -> String`、`windows::wt_command_line(...) -> String`。

- [ ] **Step 1: 写失败测试**

`src-tauri/src/platform/windows.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_line_format() {
        let s = cmd_command_line(r"D:\my dir", "npm run dev");
        assert_eq!(s, r#"/K cd /d "D:\my dir" && npm run dev"#);
    }

    #[test]
    fn powershell_line_format() {
        let s = powershell_command_line(r"D:\my dir", "python app.py");
        assert_eq!(s, "-NoExit -Command \"Set-Location 'D:\\my dir'; python app.py\"");
    }

    #[test]
    fn wt_line_format() {
        let s = wt_command_line(r"D:\my dir", "npm run dev");
        assert_eq!(s, r#"-d "D:\my dir" cmd /K npm run dev"#);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test
```

Expected: 编译失败（模块不存在）。

- [ ] **Step 3: 实现 platform**

`src-tauri/src/platform/windows.rs`：

```rust
use crate::config::Terminal;
use std::process::Command;

pub fn cmd_command_line(work_dir: &str, command: &str) -> String {
    format!("/K cd /d \"{work_dir}\" && {command}")
}

pub fn powershell_command_line(work_dir: &str, command: &str) -> String {
    format!("-NoExit -Command \"Set-Location '{work_dir}'; {command}\"")
}

pub fn wt_command_line(work_dir: &str, command: &str) -> String {
    format!("-d \"{work_dir}\" cmd /K {command}")
}

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn spawn(terminal: Terminal, work_dir: &str, command: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    match terminal {
        Terminal::Cmd => {
            let mut c = Command::new("cmd");
            c.raw_arg(cmd_command_line(work_dir, command));
            c.creation_flags(CREATE_NEW_CONSOLE);
            c.spawn().map(|_| ())
        }
        Terminal::PowerShell => {
            let mut c = Command::new("powershell");
            c.raw_arg(powershell_command_line(work_dir, command));
            c.creation_flags(CREATE_NEW_CONSOLE);
            c.spawn().map(|_| ())
        }
        Terminal::WindowsTerminal => {
            let mut c = Command::new("wt");
            c.raw_arg(wt_command_line(work_dir, command));
            c.spawn().map(|_| ())
        }
    }
}
```

`src-tauri/src/platform/mod.rs`：

```rust
#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::spawn;

#[cfg(not(windows))]
pub fn spawn(
    _terminal: crate::config::Terminal,
    _work_dir: &str,
    _command: &str,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "terminal launching is only implemented on Windows in v1",
    ))
}
```

`src-tauri/src/lib.rs` 加 `pub mod platform;`。

注意：`raw_arg` 的参数整体作为原样文本拼进命令行，`cmd /K` 与 `wt` 都把剩余内容当作命令行解析，因此含空格路径无需外层引号。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test
```

Expected: 全部 PASS（Windows 下含 3 个新测试）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: terminal platform abstraction (cmd/powershell/wt)"
```

---

### Task 4: 就绪检测 ready.rs

**Files:**
- Create: `src-tauri/src/ready.rs`
- Modify: `src-tauri/src/lib.rs`（`pub mod ready;`）

**Interfaces:**
- Consumes: `config::ReadyCondition`
- Produces: `ready::wait_ready(cond: &ReadyCondition, default_timeout_sec: u64) -> Result<(), ReadyTimeout>`；`ReadyTimeout { description: String }`；`ready::process_matches(actual: &str, wanted: &str) -> bool`。同步实现（轮询用 `std::thread::sleep`），可在任意线程调用；`timeout_sec == 0` 时回退 `default_timeout_sec`。

- [ ] **Step 1: 写失败测试**

`src-tauri/src/ready.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReadyCondition;
    use std::net::TcpListener;
    use std::time::Instant;

    #[test]
    fn immediate_is_ready() {
        assert!(wait_ready(&ReadyCondition::Immediate, 30).is_ok());
    }

    #[test]
    fn delay_waits_at_least_given_seconds() {
        let start = Instant::now();
        wait_ready(&ReadyCondition::Delay { seconds: 1 }, 30).unwrap();
        assert!(start.elapsed() >= std::time::Duration::from_millis(1000));
    }

    #[test]
    fn port_ready_when_listener_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cond = ReadyCondition::Port {
            port,
            host: "127.0.0.1".into(),
            timeout_sec: 5,
        };
        wait_ready(&cond, 30).unwrap();
    }

    #[test]
    fn port_timeout_when_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // 端口已释放，连接应失败
        let cond = ReadyCondition::Port {
            port,
            host: "127.0.0.1".into(),
            timeout_sec: 1,
        };
        assert!(wait_ready(&cond, 30).is_err());
    }

    #[test]
    fn zero_timeout_falls_back_to_default() {
        // 用一个必然关闭的端口 + 0 超时：若 0 被当作"立即超时"则瞬间失败；
        // 回退 default_timeout_sec=1 时耗时至少约 1 秒。
        let cond = ReadyCondition::Port {
            port: 1,
            host: "127.0.0.1".into(),
            timeout_sec: 0,
        };
        let start = Instant::now();
        assert!(wait_ready(&cond, 1).is_err());
        assert!(start.elapsed() >= std::time::Duration::from_millis(900));
    }

    #[cfg(windows)]
    #[test]
    fn process_found_when_running() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // 占位避免 unused import 警告
        let cond = ReadyCondition::Process {
            process_name: "explorer.exe".into(),
            timeout_sec: 5,
        };
        wait_ready(&cond, 30).unwrap();
        drop(listener);
    }

    #[test]
    fn process_matches_ignores_case_and_exe_suffix() {
        assert!(process_matches("explorer.exe", "explorer.exe"));
        assert!(process_matches("explorer.exe", "EXPLORER"));
        assert!(process_matches("python", "python.exe"));
        assert!(!process_matches("explorer.exe", "python.exe"));
    }

    #[test]
    fn process_timeout_when_not_running() {
        let cond = ReadyCondition::Process {
            process_name: "definitely-not-running-xyz123.exe".into(),
            timeout_sec: 1,
        };
        assert!(wait_ready(&cond, 30).is_err());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test
```

Expected: 编译失败。

- [ ] **Step 3: 实现 ready.rs**

```rust
use crate::config::ReadyCondition;
use std::net::TcpStream;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug)]
pub struct ReadyTimeout {
    pub description: String,
}

pub fn wait_ready(cond: &ReadyCondition, default_timeout_sec: u64) -> Result<(), ReadyTimeout> {
    match cond {
        ReadyCondition::Immediate => Ok(()),
        ReadyCondition::Delay { seconds } => {
            sleep(Duration::from_secs(*seconds));
            Ok(())
        }
        ReadyCondition::Port { port, host, timeout_sec } => {
            let addr = format!("{host}:{port}");
            poll(
                format!("端口 {addr} 不可连接"),
                *timeout_sec,
                default_timeout_sec,
                Duration::from_millis(500),
                || TcpStream::connect(&addr).is_ok(),
            )
        }
        ReadyCondition::Process { process_name, timeout_sec } => {
            let name = process_name.clone();
            poll(
                format!("进程 {process_name} 未运行"),
                *timeout_sec,
                default_timeout_sec,
                Duration::from_secs(1),
                || process_alive(&name),
            )
        }
    }
}

fn poll<F: Fn() -> bool>(
    timeout_desc: String,
    timeout_sec: u64,
    default_timeout_sec: u64,
    interval: Duration,
    check: F,
) -> Result<(), ReadyTimeout> {
    let effective = if timeout_sec == 0 { default_timeout_sec } else { timeout_sec };
    let deadline = std::time::Instant::now() + Duration::from_secs(effective);
    loop {
        if check() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(ReadyTimeout { description: timeout_desc });
        }
        sleep(interval);
    }
}

fn process_alive(wanted: &str) -> bool {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| process_matches(&p.name().to_string_lossy(), wanted))
}

pub fn process_matches(actual: &str, wanted: &str) -> bool {
    let strip_exe = |s: &str| s.strip_suffix(".exe").unwrap_or(s);
    strip_exe(actual).eq_ignore_ascii_case(strip_exe(wanted))
}

// Path 仅用于保持与未来扩展一致，当前未使用则不引入
#[allow(dead_code)]
fn _unused(_: &Path) {}
```

注意：删除 `Path` 相关的占位（上面 `_unused` 仅为说明，实际实现中不要引入未使用的 import；`use std::path::Path;` 直接删掉）。

在 `src-tauri/src/lib.rs` 加 `pub mod ready;`。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test
```

Expected: 全部 PASS（约 9 个 ready 测试，其中超时类测试各耗时 ~1s 属正常）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: readiness polling (immediate/delay/port/process)"
```

---

### Task 5: 启动编排 launcher.rs

**Files:**
- Create: `src-tauri/src/launcher.rs`
- Modify: `src-tauri/src/lib.rs`（`pub mod launcher;`）

**Interfaces:**
- Consumes: `config::{AppConfig, Project, Group, Step}`、`platform::spawn`、`ready::wait_ready`
- Produces:
  - `launcher::launch_project(app: &AppHandle, cfg: &AppConfig, project_id: &str) -> Result<(), String>`
  - `launcher::launch_group(app: &AppHandle, cfg: &AppConfig, project_id: &str, group_id: &str) -> Result<(), String>`
  - `launcher::run_step(app: &AppHandle, cfg: &AppConfig, project_id: &str, group_id: &str, step_id: &str) -> Result<(), String>`
  - `launcher::resolve_work_dir(root_dir: &str, work_dir: &Option<String>) -> std::path::PathBuf`（相对路径相对 rootDir 解析）
  - `launcher::notify(app: &AppHandle, body: String)`（系统通知）
  - 组内顺序执行：每步 spawn 后按 `ready_condition` 等待；任一步失败/超时 → 通知 + 返回 Err 中断；项目级启动依次跑完全部组。

- [ ] **Step 1: 实现 launcher.rs（以手动冒烟为验收，无单测——依赖真实终端窗口）**

```rust
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
```

- [ ] **Step 2: 编译验证**

```bash
cargo check
```

Expected: 无错误。

- [ ] **Step 3: Commit**

```bash
git add src-tauri
git commit -m "feat: launch orchestration with ready waits and notifications"
```

---

### Task 6: IPC 命令 + 托盘 + 应用装配

**Files:**
- Create: `src-tauri/src/commands.rs`
- Create: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs`（AppState、插件注册、setup、窗口事件、invoke_handler）

**Interfaces:**
- Consumes: Task 2-5 全部公开接口
- Produces（IPC，前端 `invoke` 名称与参数一致）:
  - `get_config() -> AppConfig`
  - `save_config(config: AppConfig) -> Result<(), String>`（保存后重建托盘菜单）
  - `launch_project_cmd(project_id: String) -> Result<(), String>`（后台线程执行，完成后 `emit("launch-result", Result<(), String>)`）
  - `launch_group_cmd(project_id: String, group_id: String)`（同上）
  - `run_step(project_id: String, group_id: String, step_id: String) -> Result<(), String>`
  - `open_dir(path: String) -> Result<(), String>`
  - `export_config_to(path: String) -> Result<(), String>` / `import_config_from(path: String) -> Result<(), String>`（导入后保存并重建托盘）
  - `get_autostart() -> Result<bool, String>` / `set_autostart(enabled: bool) -> Result<(), String>`
- Produces（托盘）: `tray::setup(app: &AppHandle) -> tauri::Result<()>`、`tray::rebuild(app: &AppHandle)`。托盘菜单：每项目子菜单（启动 / 打开目录）+ 打开 DevLaunch + 退出；左键单击显示主窗口。

- [ ] **Step 1: 实现 commands.rs**

```rust
use crate::launcher;
use crate::tray;
use crate::AppState;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

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
pub fn launch_group_cmd(app: AppHandle, state: State<AppState>, project_id: String, group_id: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    std::thread::spawn(move || {
        let result = launcher::launch_group(&app, &cfg, &project_id, &group_id);
        let _ = app.emit("launch-result", result.err());
    });
    Ok(())
}

#[tauri::command]
pub fn run_step(state: State<AppState>, project_id: String, group_id: String, step_id: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    launcher::run_step(&state.app_handle(), &cfg, &project_id, &group_id, &step_id)
}
```

注意：`run_step` 需要 `AppHandle`。若 `State<AppState>` 无 `app_handle()`，改为在参数中同时接收 `app: AppHandle`（Tauri 命令支持注入）。采用：

```rust
#[tauri::command]
pub fn run_step(app: AppHandle, state: State<AppState>, project_id: String, group_id: String, step_id: String) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    launcher::run_step(&app, &cfg, &project_id, &group_id, &step_id)
}
```

```rust
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
pub fn export_config_to(state: State<AppState>, path: String) -> Result<(), String> {
    state.config.lock().unwrap().save(Path::new(&path))
}

#[tauri::command]
pub fn import_config_from(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    let text = fs::read_to_string(&path).map_err(|e| format!("读取失败：{e}"))?;
    let cfg: AppConfig = serde_json::from_str(&text).map_err(|e| format!("配置文件格式错误：{e}"))?;
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

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    if enabled {
        auto.enable().map_err(|e| e.to_string())
    } else {
        auto.disable().map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 2: 实现 tray.rs**

```rust
use crate::commands;
use crate::launcher;
use crate::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

pub fn setup(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = build_menu(app)?;
    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("DevLaunch")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().0.clone()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)
}

pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let cfg = app.state::<AppState>().config.lock().unwrap().clone();
    let mut menu = Menu::new(app)?;
    for p in &cfg.projects {
        let sub = Submenu::with_id(app, format!("proj-{}", p.id), &p.name, true)?;
        sub.append(&MenuItem::with_id(app, format!("launch:{}", p.id), "启动", true, None::<&str>)?)?;
        sub.append(&MenuItem::with_id(app, format!("open:{}", p.id), "打开目录", true, None::<&str>)?)?;
        menu.append(&sub)?;
    }
    if !cfg.projects.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(app, "show", "打开 DevLaunch", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?)?;
    Ok(menu)
}

pub fn rebuild(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn handle_menu(app: &AppHandle, id: String) {
    match id.as_str() {
        "show" => show_main_window(app),
        "quit" => app.exit(0),
        _ => {}
    }
    if let Some(project_id) = id.strip_prefix("launch:") {
        let app = app.clone();
        let project_id = project_id.to_string();
        std::thread::spawn(move || {
            let cfg = app.state::<AppState>().config.lock().unwrap().clone();
            if let Err(e) = launcher::launch_project(&app, &cfg, &project_id) {
                eprintln!("launch failed: {e}");
            }
        });
    }
    if let Some(project_id) = id.strip_prefix("open:") {
        let app = app.clone();
        let project_id = project_id.to_string();
        let cfg = app.state::<AppState>().config.lock().unwrap().clone();
        if let Some(p) = cfg.projects.iter().find(|p| p.id == project_id) {
            let _ = commands::open_dir(p.root_dir.clone());
        }
    }
}
```

- [ ] **Step 3: 装配 lib.rs**

`src-tauri/src/lib.rs` 最终结构：

```rust
pub mod commands;
pub mod config;
pub mod launcher;
pub mod platform;
pub mod ready;
pub mod tray;

use config::AppConfig;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, WindowEvent};

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub path: PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("failed to resolve app data dir");
            let path = dir.join("config.json");
            let cfg = AppConfig::load(&path);
            app.manage(AppState { config: Mutex::new(cfg), path });
            tray::setup(app.handle())?;
            #[cfg(debug_assertions)]
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::launch_project_cmd,
            commands::launch_group_cmd,
            commands::run_step,
            commands::open_dir,
            commands::export_config_to,
            commands::import_config_from,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

模板中原有的 greet 命令/示例代码删除。

- [ ] **Step 4: 编译并手动冒烟**

```bash
cargo check
npm run tauri dev
```

手动验证：
1. 启动后出现托盘图标（默认项目为空，菜单只有"打开 DevLaunch/退出"）
2. dev 模式下主窗口自动显示；点窗口 X → 窗口隐藏且进程仍在（托盘还在）
3. 托盘左键单击 → 主窗口显示；右键 → 菜单；"退出" → 进程结束

- [ ] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: IPC commands, tray menu, app wiring (hide on close)"
```

---

### Task 7: 前端类型 + API + 应用外壳 + 快速启动主页

**Files:**
- Create: `src/types.ts`
- Create: `src/api.ts`
- Create: `src/store.ts`
- Modify: `src/App.vue`（整体替换模板示例）
- Create: `src/views/HomeView.vue`
- Modify: `src/style.css`（整体替换）
- Delete: 模板自带的 `src/components/`（HelloWorld 等示例，如存在）

**Interfaces:**
- Consumes: Task 6 的全部 IPC 命令与 `launch-result` 事件
- Produces: `types.ts` 中与 serde camelCase 一致的 TS 类型（`AppConfig/Project/Group/Step/Terminal/ReadyCondition/Settings`）；`store.ts` 导出模块级单例 `config: Ref<AppConfig | null>`、`load(): Promise<void>`、`persist(): Promise<void>`；`api.ts` 封装全部 invoke。

- [ ] **Step 1: 写 types.ts**

```ts
export type Terminal = 'cmd' | 'powershell' | 'windowsterminal'

export type ReadyCondition =
  | { type: 'immediate' }
  | { type: 'delay'; seconds: number }
  | { type: 'port'; port: number; host: string; timeoutSec: number }
  | { type: 'process'; processName: string; timeoutSec: number }

export interface Step {
  id: string
  name: string
  workDir?: string | null
  terminal: Terminal
  command: string
  readyCondition: ReadyCondition
}

export interface Group {
  id: string
  name: string
  steps: Step[]
}

export interface Project {
  id: string
  name: string
  rootDir: string
  groups: Group[]
}

export interface Settings {
  readyTimeoutSec: number
  autostart: boolean
}

export interface AppConfig {
  version: number
  settings: Settings
  projects: Project[]
}

export function newId(): string {
  return crypto.randomUUID()
}

export function newStep(): Step {
  return { id: newId(), name: '', workDir: '', terminal: 'cmd', command: '', readyCondition: { type: 'immediate' } }
}

export function newGroup(index: number): Group {
  return { id: newId(), name: `组 ${index + 1}`, steps: [] }
}
```

- [ ] **Step 2: 写 api.ts 与 store.ts**

`src/api.ts`：

```ts
import { invoke } from '@tauri-apps/api/core'
import type { AppConfig } from './types'

export const getConfig = () => invoke<AppConfig>('get_config')
export const saveConfig = (config: AppConfig) => invoke<void>('save_config', { config })
export const launchProject = (projectId: string) => invoke<void>('launch_project_cmd', { projectId })
export const launchGroup = (projectId: string, groupId: string) =>
  invoke<void>('launch_group_cmd', { projectId, groupId })
export const runStep = (projectId: string, groupId: string, stepId: string) =>
  invoke<void>('run_step', { projectId, groupId, stepId })
export const openDir = (path: string) => invoke<void>('open_dir', { path })
export const exportConfigTo = (path: string) => invoke<void>('export_config_to', { path })
export const importConfigFrom = (path: string) => invoke<void>('import_config_from', { path })
export const getAutostart = () => invoke<boolean>('get_autostart')
export const setAutostart = (enabled: boolean) => invoke<void>('set_autostart', { enabled })
```

`src/store.ts`：

```ts
import { ref, type Ref } from 'vue'
import type { AppConfig } from './types'
import { getConfig, saveConfig } from './api'

export const config: Ref<AppConfig | null> = ref(null)

export async function load(): Promise<void> {
  config.value = await getConfig()
}

export async function persist(): Promise<void> {
  if (config.value) {
    await saveConfig(JSON.parse(JSON.stringify(config.value)))
  }
}
```

- [ ] **Step 3: 写 App.vue 与 HomeView.vue**

`src/App.vue`：

```vue
<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { load, config } from './store'
import HomeView from './views/HomeView.vue'
import EditorView from './views/EditorView.vue'
import SettingsView from './views/SettingsView.vue'

export type View = { name: 'home' } | { name: 'editor'; projectId: string } | { name: 'settings' }

const view = ref<View>({ name: 'home' })
const toast = ref('')
let toastTimer: number | undefined

function showToast(msg: string) {
  toast.value = msg
  clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => (toast.value = ''), 5000)
}

onMounted(async () => {
  await load()
  await listen<string | null>('launch-result', (e) => {
    if (e.payload) showToast(`启动中断：${e.payload}`)
    else showToast('启动完成')
  })
})
</script>

<template>
  <div class="app" v-if="config">
    <nav>
      <button :class="{ active: view.name === 'home' }" @click="view = { name: 'home' }">项目</button>
      <button :class="{ active: view.name === 'settings' }" @click="view = { name: 'settings' }">设置</button>
    </nav>
    <main>
      <HomeView v-if="view.name === 'home'" @edit="view = { name: 'editor', projectId: $event }" @notify="showToast" />
      <EditorView v-else-if="view.name === 'editor'" :project-id="view.projectId" @back="view = { name: 'home' }" @notify="showToast" />
      <SettingsView v-else @notify="showToast" />
    </main>
    <div v-if="toast" class="toast">{{ toast }}</div>
  </div>
</template>
```

`src/views/HomeView.vue`：

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { config } from '../store'
import { launchProject, openDir } from '../api'

const emit = defineEmits<{ edit: [projectId: string]; notify: [msg: string] }>()

const projects = computed(() => config.value?.projects ?? [])

async function launch(id: string) {
  try {
    await launchProject(id)
    emit('notify', '已开始启动…')
  } catch (e) {
    emit('notify', `启动失败：${e}`)
  }
}

async function open(path: string) {
  try {
    await openDir(path)
  } catch (e) {
    emit('notify', `${e}`)
  }
}
</script>

<template>
  <div class="home">
    <h1>项目</h1>
    <p v-if="projects.length === 0" class="empty">还没有项目。点击右上角「新建项目」开始配置。</p>
    <div v-for="p in projects" :key="p.id" class="project-row">
      <span class="name">{{ p.name }}</span>
      <span class="actions">
        <button class="primary" @click="launch(p.id)">启动</button>
        <button @click="open(p.rootDir)">打开目录</button>
        <button @click="emit('edit', p.id)">编辑</button>
      </span>
    </div>
    <button
      v-if="config"
      class="primary add"
      @click="config.projects.push({ id: crypto.randomUUID(), name: '新项目', rootDir: '', groups: [] }); emit('edit', config.projects[config.projects.length - 1].id)"
    >
      新建项目
    </button>
  </div>
</template>
```

- [ ] **Step 4: 写 style.css（整体替换模板样式）**

```css
:root {
  color-scheme: dark;
  font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
  --bg: #1e1f24;
  --panel: #2a2b31;
  --border: #3a3b42;
  --text: #e6e6e9;
  --muted: #9a9aa3;
  --accent: #4f8cff;
}

* { box-sizing: border-box; }

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
}

.app { max-width: 880px; margin: 0 auto; padding: 16px; }

nav {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

nav button, .app button {
  background: var(--panel);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 14px;
  cursor: pointer;
  font-size: 14px;
}

nav button.active { border-color: var(--accent); color: var(--accent); }

button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
button.danger { color: #ff6b6b; }

.project-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 16px;
  margin-bottom: 8px;
}

.project-row .actions { display: flex; gap: 8px; }
.project-row .name { font-size: 16px; font-weight: 600; }

.empty { color: var(--muted); }

.add { margin-top: 8px; }

input, select {
  background: var(--bg);
  border: 1px solid var(--border);
  color: var(--text);
  border-radius: 4px;
  padding: 5px 8px;
  font-size: 13px;
}

.group-card, .step-row {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 16px;
  margin-bottom: 10px;
}

.step-row {
  display: grid;
  grid-template-columns: 110px 1fr 1fr 130px 1fr auto;
  gap: 8px;
  align-items: center;
}

.step-row input, .step-row select { width: 100%; }

.cond-fields { grid-column: 1 / -1; display: flex; gap: 8px; align-items: center; }

label { font-size: 13px; color: var(--muted); display: flex; flex-direction: column; gap: 4px; }

.toast {
  position: fixed;
  bottom: 16px;
  left: 50%;
  transform: translateX(-50%);
  background: var(--panel);
  border: 1px solid var(--border);
  padding: 10px 18px;
  border-radius: 8px;
}

h1 { font-size: 20px; } h2 { font-size: 16px; }
.hint { color: var(--muted); font-size: 12px; margin: 4px 0 10px; }
.row { display: flex; gap: 8px; align-items: center; }
.grow { flex: 1; }
```

- [ ] **Step 5: 类型检查与构建**

```bash
npm run build
```

Expected: vue-tsc 无错误（EditorView/SettingsView 尚未创建，此步在 Task 8/9 之前应先建空占位组件以通过编译：创建 `src/views/EditorView.vue` 与 `src/views/SettingsView.vue`，内容仅 `<template><div /></template>`，Task 8/9 再填充）。

- [ ] **Step 6: 手动冒烟（dev 模式）**

```bash
npm run tauri dev
```

验证：主窗口显示项目页；新建项目 → 跳编辑器（空壳）；托盘菜单随保存重建；toast 显示正常。

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: frontend shell, quick-launch home, types and IPC api"
```

---

### Task 8: 项目编辑器（分组/步骤/就绪条件/单步运行）

**Files:**
- Modify: `src/views/EditorView.vue`（替换 Task 7 的空占位）

**Interfaces:**
- Consumes: `store.config/persist`、`api.launchGroup/runStep/openDir`、`types.ts` 的 `newGroup/newStep/newId`
- Produces: 完整编辑器视图；保存即写盘并重建托盘菜单

- [ ] **Step 1: 实现 EditorView.vue**

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
import { config, persist } from '../store'
import { launchGroup, openDir, runStep } from '../api'
import { newGroup, newStep, type Group, type ReadyCondition, type Step, type Terminal } from '../types'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ back: []; notify: [msg: string] }>()

const project = computed(() => config.value!.projects.find((p) => p.id === props.projectId)!)

async function save() {
  try {
    await persist()
    emit('notify', '已保存')
  } catch (e) {
    emit('notify', `保存失败：${e}`)
  }
}

function addGroup() {
  project.value.groups.push(newGroup(project.value.groups.length))
}

function removeGroup(gi: number) {
  project.value.groups.splice(gi, 1)
}

function addStep(g: Group) {
  g.steps.push(newStep())
}

function removeStep(g: Group, si: number) {
  g.steps.splice(si, 1)
}

function moveStep(g: Group, si: number, dir: number) {
  const j = si + dir
  if (j < 0 || j >= g.steps.length) return
  ;[g.steps[si], g.steps[j]] = [g.steps[j], g.steps[si]]
}

function onCondTypeChange(step: Step, t: string) {
  const timeout = config.value!.settings.readyTimeoutSec
  if (t === 'immediate') step.readyCondition = { type: 'immediate' }
  else if (t === 'delay') step.readyCondition = { type: 'delay', seconds: 3 }
  else if (t === 'port') step.readyCondition = { type: 'port', port: 8000, host: '127.0.0.1', timeoutSec: timeout }
  else step.readyCondition = { type: 'process', processName: '', timeoutSec: timeout }
}

const terminals: Terminal[] = ['cmd', 'powershell', 'windowsterminal']

async function tryRunStep(g: Group, s: Step) {
  try {
    await runStep(project.value.id, g.id, s.id)
  } catch (e) {
    emit('notify', `${e}`)
  }
}

async function tryRunGroup(g: Group) {
  try {
    await launchGroup(project.value.id, g.id)
    emit('notify', '分组已开始启动…')
  } catch (e) {
    emit('notify', `启动失败：${e}`)
  }
}

async function browseRoot() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ directory: true, multiple: false })
  if (typeof picked === 'string') project.value.rootDir = picked
}
</script>

<template>
  <div class="editor" v-if="project">
    <div class="row">
      <button @click="emit('back')">← 返回</button>
      <h1 class="grow">{{ project.name || '未命名项目' }}</h1>
      <button class="primary" @click="save">保存</button>
    </div>

    <div class="group-card">
      <label>项目名称 <input v-model="project.name" /></label>
      <label style="margin-top: 8px">根目录
        <span class="row">
          <input class="grow" v-model="project.rootDir" />
          <button @click="browseRoot">选择…</button>
        </span>
      </label>
    </div>

    <div v-for="(g, gi) in project.groups" :key="g.id" class="group-card">
      <div class="row">
        <input v-model="g.name" class="grow" />
        <button @click="tryRunGroup(g)">运行本组</button>
        <button class="danger" @click="removeGroup(gi)">删除组</button>
      </div>

      <div v-for="(s, si) in g.steps" :key="s.id" class="step-row" style="margin-top: 10px">
        <input v-model="s.name" placeholder="名称" />
        <input v-model="s.command" placeholder="命令，如 npm run dev" />
        <input v-model="s.workDir" placeholder="子目录（留空=根目录）" />
        <select v-model="s.terminal">
          <option value="cmd">CMD</option>
          <option value="powershell">PowerShell</option>
          <option value="windowsterminal">Windows Terminal</option>
        </select>
        <select :value="s.readyCondition.type" @change="onCondTypeChange(s, ($event.target as HTMLSelectElement).value)">
          <option value="immediate">就绪：立即</option>
          <option value="delay">就绪：延迟</option>
          <option value="port">就绪：端口</option>
          <option value="process">就绪：进程</option>
        </select>
        <span class="row">
          <button @click="moveStep(g, si, -1)" :disabled="si === 0">↑</button>
          <button @click="moveStep(g, si, 1)" :disabled="si === g.steps.length - 1">↓</button>
          <button @click="tryRunStep(g, s)">运行</button>
          <button class="danger" @click="removeStep(g, si)">✕</button>
        </span>

        <div class="cond-fields" v-if="s.readyCondition.type === 'delay'">
          <label>等待秒数 <input type="number" v-model.number="(s.readyCondition as any).seconds" min="0" /></label>
        </div>
        <div class="cond-fields" v-else-if="s.readyCondition.type === 'port'">
          <label>主机 <input v-model="(s.readyCondition as any).host" /></label>
          <label>端口 <input type="number" v-model.number="(s.readyCondition as any).port" min="1" max="65535" /></label>
          <label>超时秒 <input type="number" v-model.number="(s.readyCondition as any).timeoutSec" min="0" /></label>
        </div>
        <div class="cond-fields" v-else-if="s.readyCondition.type === 'process'">
          <label>进程名（如 python.exe，不要填终端自身）<input v-model="(s.readyCondition as any).processName" /></label>
          <label>超时秒 <input type="number" v-model.number="(s.readyCondition as any).timeoutSec" min="0" /></label>
        </div>
      </div>

      <div style="margin-top: 10px">
        <button @click="addStep(g)">+ 添加步骤</button>
      </div>
    </div>

    <button class="add" @click="addGroup">+ 添加分组</button>
    <p class="hint">组内按顺序启动：上一步按"就绪条件"等待后再启动下一步；组与组之间在首页逐组手动运行。就绪条件类型切换后请重新填写参数。</p>
  </div>
</template>
```

- [ ] **Step 2: 类型检查**

```bash
npm run build
```

Expected: vue-tsc 通过。若 `(s.readyCondition as any)` 引发 lint 报错可忽略（无 lint 配置），vue-tsc 下 `any` 合法。

- [ ] **Step 3: 手动冒烟**

`npm run tauri dev`，验证：新建项目/组/步骤、切换就绪条件类型、上下移动步骤、单步运行（弹出真实终端并执行命令）、运行本组、保存后托盘菜单出现该项目子菜单。

- [ ] **Step 4: Commit**

```bash
git add src
git commit -m "feat: project editor with groups, steps and ready conditions"
```

---

### Task 9: 设置页（超时/开机自启/导入导出）

**Files:**
- Modify: `src/views/SettingsView.vue`（替换空占位）

**Interfaces:**
- Consumes: `api.getAutostart/setAutostart/exportConfigTo/importConfigFrom/getConfig`、`store.config/persist`
- Produces: 设置视图；导入成功后刷新 `store.config`

- [ ] **Step 1: 实现 SettingsView.vue**

```vue
<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { config, persist } from '../store'
import { exportConfigTo, getAutostart, getConfig, importConfigFrom, setAutostart } from '../api'

const emit = defineEmits<{ notify: [msg: string] }>()

const autostart = ref(false)

onMounted(async () => {
  try {
    autostart.value = await getAutostart()
  } catch {
    autostart.value = false
  }
})

async function toggleAutostart() {
  try {
    await setAutostart(autostart.value)
    emit('notify', autostart.value ? '已开启开机自启' : '已关闭开机自启')
  } catch (e) {
    autostart.value = !autostart.value
    emit('notify', `设置失败：${e}`)
  }
}

async function saveTimeout() {
  await persist()
  emit('notify', '默认超时已保存')
}

async function doExport() {
  const { save } = await import('@tauri-apps/plugin-dialog')
  const path = await save({
    defaultPath: 'devlaunch-config.json',
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (!path) return
  try {
    await exportConfigTo(path)
    emit('notify', '配置已导出')
  } catch (e) {
    emit('notify', `导出失败：${e}`)
  }
}

async function doImport() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    multiple: false,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (typeof picked !== 'string') return
  try {
    await importConfigFrom(picked)
    config.value = await getConfig()
    emit('notify', '配置已导入')
  } catch (e) {
    emit('notify', `导入失败：${e}`)
  }
}
</script>

<template>
  <div class="settings" v-if="config">
    <h1>设置</h1>

    <div class="group-card">
      <label>默认就绪超时（秒）
        <input type="number" v-model.number="config.settings.readyTimeoutSec" min="1" @change="saveTimeout" />
      </label>
    </div>

    <div class="group-card row">
      <input type="checkbox" v-model="autostart" @change="toggleAutostart" id="autostart" />
      <label for="autostart" style="flex-direction: row">开机自动启动 DevLaunch（托盘常驻）</label>
    </div>

    <div class="group-card row">
      <button @click="doExport">导出配置</button>
      <button @click="doImport">导入配置</button>
    </div>

    <p class="hint">配置文件位置：%APPDATA%\com.devlaunch.app\config.json</p>
  </div>
</template>
```

- [ ] **Step 2: 类型检查**

```bash
npm run build
```

Expected: 通过。

- [ ] **Step 3: 手动冒烟**

`npm run tauri dev`，验证：修改默认超时并保存（`config.json` 中 `readyTimeoutSec` 变化）；勾选/取消自启（`reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 出现/消失 DevLaunch 条目）；导出 → 改乱本地配置 → 导入还原。

- [ ] **Step 4: Commit**

```bash
git add src
git commit -m "feat: settings view (timeout, autostart, import/export)"
```

---

### Task 10: 端到端验收 + 发布构建

**Files:**
- Modify: `src-tauri/tauri.conf.json`（如需图标/产品名修正）

**Interfaces:**
- Consumes: 全部已完成任务
- Produces: 通过验收清单的可发布应用

- [ ] **Step 1: 端到端手动验收清单**

用真实项目（如 PVDS 示例结构：server/python + web/npm）配置并逐项验证：

1. 新建项目 PVDS，根目录选择真实目录，三个步骤（python app.py / npm run dev / opencode），全部 PowerShell
2. 首页点「启动」→ 依次弹出 3 个真实终端窗口，命令自动执行，窗口可交互
3. Step 1 就绪条件设为端口（填真实服务端口）→ 服务起来后 Step 2 才启动
4. Step 1 端口填一个永不开启的端口 + 1 秒超时 → 收到系统通知，Step 2/3 未启动，已开窗口不受影响
5. 步骤 1 的命令改成不存在的命令 → 终端窗口保留并显示错误
6. 步骤目录填不存在路径 → 系统通知报错，链式启动停止
7. 托盘子菜单「启动」与「打开目录」均生效
8. 关闭主窗口 → 进程仍在托盘；托盘退出 → 进程结束
9. 重启应用 → 配置完整还原
10. CMD / PowerShell / Windows Terminal 三种终端各验证一次（wt 未安装时应收到错误通知而非崩溃）

- [ ] **Step 2: 修复验收中发现的问题**

- [ ] **Step 3: 发布构建**

```bash
npm run tauri build
```

Expected: `src-tauri/target/release/` 生成 DevLaunch.exe 与安装包；运行 release 版完整通过清单 2、3、4、7、8 项。

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: e2e acceptance pass and release build config"
```

---

## 自审记录

- **Spec 覆盖**：数据模型(T2)、四种就绪条件(T4)、三终端抽象(T3)、编排队列与超时中断(T5)、托盘启动/打开目录/退出(T6)、窗口隐藏(T6)、配置原子写+损坏备份(T2)、导入导出(T6/T9)、自启(T6/T9)、前端编辑器(T7/T8)、默认组=单组项目直接可用(T8 允许 0 组/单组)、错误不吞(T3 /K 与 -NoExit + T5 通知) —— 均有对应任务。
- **占位符**：无 TBD/TODO；所有代码步骤给出完整代码。
- **类型一致性**：`ReadyCondition` 的 serde tag/字段（seconds/port/host/processName/timeoutSec）在 config.rs、types.ts、编辑器 UI 三处一致；`launch_project_cmd`/`launch_group_cmd`/`run_step`/`open_dir`/`export_config_to`/`import_config_from`/`get_autostart`/`set_autostart` 命令名在 commands.rs 与 api.ts 一致；`launch-result` 事件载荷为 `Result<(), String>` 的 err 侧（`string | null`）与 App.vue 监听一致。
- **已知取舍**：左键单击打开主窗口（主页即快速启动列表），而非独立悬浮面板——V1 简化，符合"点一两个按钮"的核心诉求；wt 内固定用 cmd /K 作为内层 shell。

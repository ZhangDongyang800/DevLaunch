# DevLaunch v3 最小架构 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 DevLaunch 从「步骤 + 门控 + 脚本编排」重构为「一键重放器」：项目 → 启动项[]，一次 `wt` 调用出一个窗口多窗格，Rust 不等待不监控。

**Architecture:** 每个启动项 = 一个真实终端里的多行命令（同一 shell 会话）。`platform` 层负责将启动项构造为 cmd / PowerShell 命令，优先用一条 `wt` 命令行创建「一个窗口 + N 个 pane」，无 wt 时降级为每项独立窗口。`launcher` 只做校验、构造、spawn、通知，spawn 成功即结束。

**Tech Stack:** Tauri 2 + Rust（无 async），Vue 3 + TS + Vite；唯一新增依赖 `base64`（PowerShell `-EncodedCommand`）。

**Spec:** `docs/superpowers/specs/2026-09-10-devlaunch-v3-minimal-design.md`

## Global Constraints

- Windows only。真实终端、真实 shell 会话；禁止等待 / 轮询 / 监控 / 状态机 / 哨兵 / 编排线程。
- 产品层概念只允许：项目、启动项（名称 / workDir / command / shell）。禁止出现 group / step / pane / readyCondition / prelude / finish / process。
- `CONFIG_VERSION = 3`；v1 / v2 配置与模板必须自动迁移（内存迁移，下次保存落盘）。
- 优先 Windows Terminal：一次 `wt` 调用一个窗口、每启动项一个 pane；wt 不存在时降级为每项独立终端窗口，语义不变。
- 每个任务结束必须通过：前端 `npm run build`（vue-tsc + vite）、后端在 `src-tauri/` 下 `cargo test`。最终统一跑一次 `npm run tauri build`（AGENTS.md 构建纪律）。
- 每个任务结束提交一次 commit；不要提交与本任务无关的改动。
- **前置条件**：当前工作区有未提交的 v2 迭代改动（`git status` 可见）。Task 1 第一步必须先提交或 stash 这些改动，保证后续每个任务从干净工作区开始。

## 现有代码审计（复用 / 删除）

| 文件 | 处置 | 说明 |
|---|---|---|
| `src-tauri/src/config.rs` | 重写模型 + 迁移 | 保留：原子写 `save_json`、`backup_corrupt`、`AppConfig::load` 的损坏回退、`save`。重写：`Shell` / `Item` / `Project.items` / `Settings{autostart}` / `ProjectTemplate.items` / `CONFIG_VERSION=3` / legacy 迁移（v1/v2 → v3，用私有 Legacy 结构反序列化）。删除：`ReadyCondition`、`Step`、`Group`、`Terminal`、`ready_timeout_sec`、`migrate_v1` / `migrate_group_terminal`。 |
| `src-tauri/src/launcher.rs` | 重写 | 删除：`build_group_script`、`cd_line`、`call_prefixed`、`script_ext`、`script_path_for_group`、`write_group_script`、`write_script_file`、`launch_group`、`run_step` 及相关单测。保留：`notify`、`resolve_work_dir`。新增：`launch_project` / `launch_item` / `launch_items` / 纯函数 `build_panes`。 |
| `src-tauri/src/ready.rs` | 删除 | 整个文件与 `lib.rs` 中 `pub mod ready;`。 |
| `src-tauri/src/platform/windows.rs` | 重写 | 删除：`join_lines`（迁为 `fold_cmd_lines`）、`cmd_script_args`、`ps_script_args`、`wt_script_args`、`spawn_script`。新增：`PaneSpec`、`LaunchMode`、`fold_cmd_lines`、`cmd_pane_command`、`ps_pane_script`、`encode_ps_command`、`resolve_wt_with` / `resolve_wt_path`、`build_wt_commandline`、`cmd_launch_args` / `ps_launch_args`、`spawn_panes` / `spawn_fallback`。 |
| `src-tauri/src/platform/mod.rs` | 改 | 导出新 API；非 Windows 保留 Unsupported stub。 |
| `src-tauri/src/commands.rs` | 改 | 删除：`launch_group_cmd`、`run_step`。新增：`launch_item_cmd`、`export_project_file`。`launch_project_cmd` 改为同步返回 Result（不再后台线程 + 事件）。`import_config_from` 改用 `config::parse_config`。保留：config 读写、`list_subdirs`、`open_dir`、import/export、autostart。 |
| `src-tauri/src/lib.rs` | 小改 | 移除 `pub mod ready;`；更新 `invoke_handler` 列表。 |
| `src-tauri/src/tray.rs` | 不改 | 继续调用 `launcher::launch_project`（签名不变）。 |
| `src-tauri/Cargo.toml` | 改 | 加 `base64 = "0.22"`；Task 2 删除 `encoding_rs`。 |
| `src/types.ts` | 重写 | `Shell` / `Item` / `Project.items` / `Settings{autostart}` / `ProjectTemplate.items` / `newItem()`。 |
| `src/api.ts` | 改 | 删 `launchGroup` / `runStep`；加 `launchItem` / `exportProjectFile`。 |
| `src/App.vue` | 小改 | 删 `launch-result` 事件监听（启动命令改为同步返回）。 |
| `src/views/HomeView.vue` | 改 | 管道渲染（steps/gates）→ 启动项列表渲染；`createProject` 用 `items: []`。 |
| `src/views/EditorView.vue` | 重写 | 启动项编辑器（名称 / 目录下拉 / 多行命令 / 高级 shell / 单启 / 排序 / 导入导出）。 |
| `src/views/SettingsView.vue` | 小改 | 删「默认就绪超时」。 |
| `src/style.css` | 小改 | 删除仅被移除的 gate / group 时间线使用的规则。 |
| `docs/PRODUCT.md`、`AGENTS.md` | 改 | Task 4 文档同步。 |

---

## Task 1: platform 投递层（纯函数 + 可测试，不动现有调用方）

**Files:**
- Modify: `src-tauri/src/config.rs`（仅新增 `Shell`，不动旧类型）
- Modify: `src-tauri/src/platform/windows.rs`
- Modify: `src-tauri/src/platform/mod.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: 无（当前代码全部保持可用）
- Produces:
  - `config::Shell { Cmd, PowerShell }`（serde lowercase，默认 Cmd）
  - `platform::PaneSpec { title: String, work_dir: PathBuf, shell: Shell, command: String }`
  - `platform::LaunchMode { WindowsTerminal, Fallback }`
  - `platform::fold_cmd_lines(&str) -> String`
  - `platform::cmd_pane_command(&Path, &str) -> String`
  - `platform::ps_pane_script(&Path, &str) -> String`
  - `platform::encode_ps_command(&str) -> String`
  - `platform::resolve_wt_with(Option<&OsStr>, Option<&OsStr>, Option<&OsStr>) -> Option<PathBuf>`
  - `platform::resolve_wt_path() -> Option<PathBuf>`
  - `platform::build_wt_commandline(&str, &[PaneSpec]) -> String`
  - `platform::validate_wt_commandline(&str) -> Result<(), String>`
  - `platform::cmd_launch_args(&Path, &str) -> String` / `platform::ps_launch_args(&Path, &str) -> String`
  - `platform::spawn_panes(&str, &[PaneSpec]) -> io::Result<LaunchMode>`

- [ ] **Step 0: 处理未提交改动**

```bash
git status
# 将现有 v2 未提交改动提交（git add -A && git commit -m "wip: v2 iteration"）或 stash；保持工作区干净
```

- [ ] **Step 1: 加 base64 依赖**

```toml
# src-tauri/Cargo.toml [dependencies]
base64 = "0.22"
```

Run: `cargo test`（在 `src-tauri/` 下）→ 通过（基线 32 个测试保持绿）

- [ ] **Step 2: 写失败的 Shell 序列化测试**

在 `config.rs` 测试模块加：

```rust
#[test]
fn shell_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&Shell::Cmd).unwrap(), "\"cmd\"");
    assert_eq!(serde_json::to_string(&Shell::PowerShell).unwrap(), "\"powershell\"");
    assert_eq!(Shell::default(), Shell::Cmd);
}
```

Run: `cargo test shell_serializes_lowercase` → FAIL（`Shell` 未定义，编译错误即失败）

- [ ] **Step 3: 实现 Shell**

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    #[default]
    Cmd,
    PowerShell,
}
```

Run: `cargo test shell_serializes_lowercase` → PASS

- [ ] **Step 4: 写 platform 判断命令构造的失败测试**

`windows.rs` 测试模块加：

```rust
fn pane(title: &str, dir: &str, shell: Shell, cmd: &str) -> PaneSpec {
    PaneSpec { title: title.into(), work_dir: PathBuf::from(dir), shell, command: cmd.into() }
}

#[test]
fn folds_cmd_lines_and_keeps_ps_multiline() {
    assert_eq!(fold_cmd_lines("a\n  b \n\nc"), "a && b && c");
    assert_eq!(fold_cmd_lines("\n \n"), "");
}

#[test]
fn cmd_pane_command_cd_and_chain() {
    assert_eq!(
        cmd_pane_command(Path::new(r"D:\My Proj\backend"), "conda activate x\npython -m uvicorn main:app"),
        r#"cd /d "D:\My Proj\backend" && conda activate x && python -m uvicorn main:app"#
    );
    assert_eq!(cmd_pane_command(Path::new(r"D:\p"), "  "), r#"cd /d "D:\p""#);
}

#[test]
fn ps_pane_script_quotes_and_multiline() {
    assert_eq!(
        ps_pane_script(Path::new(r"D:\it's"), "npm run dev\nnpm test"),
        "Set-Location -LiteralPath 'D:\\it''s'\r\nnpm run dev\nnpm test"
    );
}

#[test]
fn ps_encoded_command_is_utf16le_base64() {
    assert_eq!(encode_ps_command("hi"), "aABpAA==");
}

#[test]
fn wt_commandline_first_tab_then_splits() {
    let panes = vec![
        pane("后端", r"D:\p\backend", Shell::Cmd, "python app.py"),
        pane("前端", r"D:\p\frontend", Shell::PowerShell, "npm run dev"),
    ];
    let line = build_wt_commandline("XingTu", &panes);
    assert_eq!(
        line,
        format!(
            r#"-w -1 nt -d "D:\p\backend" --title "XingTu" --suppressApplicationTitle cmd /K "cd /d "D:\p\backend" && python app.py" ; sp -V -d "D:\p\frontend" --title "前端" --suppressApplicationTitle powershell -NoExit -ExecutionPolicy Bypass -EncodedCommand {}"#,
            encode_ps_command(&ps_pane_script(Path::new(r"D:\p\frontend"), "npm run dev"))
        )
    );
}

#[test]
fn launch_args_for_fallback_windows() {
    assert_eq!(cmd_launch_args(Path::new(r"D:\p"), "npm run dev"), r#"/K "cd /d "D:\p" && npm run dev""#);
    assert!(ps_launch_args(Path::new(r"D:\p"), "npm run dev").starts_with("-NoExit -ExecutionPolicy Bypass -EncodedCommand "));
}

#[test]
fn resolve_wt_prefers_env_override() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("wt.exe");
    std::fs::write(&fake, "x").unwrap();
    let got = resolve_wt_with(
        Some(fake.as_os_str()),
        None,
        None,
    );
    assert_eq!(got.as_deref(), Some(fake.as_path()));
}

#[test]
fn resolve_wt_empty_override_forces_none() {
    // 空字符串 = 强制禁用 wt（用于验证降级路径）
    assert_eq!(resolve_wt_with(Some(OsStr::new("")), None, None), None);
}

#[test]
fn rejects_overlong_commandline() {
    assert!(validate_wt_commandline(&"a".repeat(30_001)).is_err());
    assert!(validate_wt_commandline("short").is_ok());
}
```

Run: `cargo test --lib platform` → FAIL（新函数/类型未定义）

- [ ] **Step 5: 实现 platform 新函数**

`windows.rs` 新增（保留旧函数到 Task 2 再删）：

```rust
use crate::config::Shell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct PaneSpec {
    pub title: String,
    pub work_dir: PathBuf,
    pub shell: Shell,
    pub command: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchMode {
    WindowsTerminal,
    Fallback,
}

pub fn fold_cmd_lines(command: &str) -> String {
    command.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" && ")
}

pub fn cmd_pane_command(work_dir: &Path, command: &str) -> String {
    let cd = format!("cd /d \"{}\"", work_dir.display());
    let folded = fold_cmd_lines(command);
    if folded.is_empty() { cd } else { format!("{cd} && {folded}") }
}

pub fn ps_pane_script(work_dir: &Path, command: &str) -> String {
    let cd = format!(
        "Set-Location -LiteralPath '{}'",
        work_dir.display().to_string().replace('\'', "''")
    );
    if command.trim().is_empty() { cd } else { format!("{cd}\r\n{command}") }
}

pub fn encode_ps_command(script: &str) -> String {
    use base64::Engine;
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn sanitize_title(title: &str) -> String {
    title.replace('"', "'")
}

pub fn resolve_wt_with(
    override_path: Option<&OsStr>,
    path_env: Option<&OsStr>,
    local_appdata: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if p.is_empty() {
            return None;
        }
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let cand = dir.join("wt.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let local = local_appdata?;
    let cand = PathBuf::from(local).join("Microsoft").join("WindowsApps").join("wt.exe");
    cand.is_file().then_some(cand)
}

pub fn resolve_wt_path() -> Option<PathBuf> {
    resolve_wt_with(
        std::env::var_os("DEVLAUNCH_WT_PATH").as_deref(),
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

pub fn build_wt_commandline(project_name: &str, panes: &[PaneSpec]) -> String {
    let mut line = String::from("-w -1");
    for (i, p) in panes.iter().enumerate() {
        let title = if i == 0 { project_name } else { &p.title };
        if i == 0 {
            line.push_str(&format!(
                " nt -d \"{}\" --title \"{}\"",
                p.work_dir.display(),
                sanitize_title(title)
            ));
        } else {
            line.push_str(&format!(
                " ; sp -V -d \"{}\" --title \"{}\"",
                p.work_dir.display(),
                sanitize_title(title)
            ));
        }
        line.push_str(" --suppressApplicationTitle");
        match p.shell {
            Shell::Cmd => {
                line.push_str(" cmd /K \"");
                line.push_str(&cmd_pane_command(&p.work_dir, &p.command));
                line.push('"');
            }
            Shell::PowerShell => {
                line.push_str(" powershell -NoExit -ExecutionPolicy Bypass -EncodedCommand ");
                line.push_str(&encode_ps_command(&ps_pane_script(&p.work_dir, &p.command)));
            }
        }
    }
    line
}

pub fn cmd_launch_args(work_dir: &Path, command: &str) -> String {
    format!("/K \"{}\"", cmd_pane_command(work_dir, command))
}

pub fn ps_launch_args(work_dir: &Path, command: &str) -> String {
    format!(
        "-NoExit -ExecutionPolicy Bypass -EncodedCommand {}",
        encode_ps_command(&ps_pane_script(work_dir, command))
    )
}

pub fn validate_wt_commandline(line: &str) -> Result<(), String> {
    if line.chars().count() > 30_000 {
        Err("启动项命令总长度超过 30000 字符，请拆分启动项".into())
    } else {
        Ok(())
    }
}

pub fn spawn_panes(project_name: &str, panes: &[PaneSpec]) -> std::io::Result<LaunchMode> {
    if let Some(wt) = resolve_wt_path() {
        let line = build_wt_commandline(project_name, panes);
        validate_wt_commandline(&line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        Command::new(wt).raw_arg(line).spawn()?;
        Ok(LaunchMode::WindowsTerminal)
    } else {
        spawn_fallback(panes)?;
        Ok(LaunchMode::Fallback)
    }
}

fn spawn_fallback(panes: &[PaneSpec]) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    for p in panes {
        let mut c = match p.shell {
            Shell::Cmd => {
                let mut c = Command::new("cmd");
                c.raw_arg(cmd_launch_args(&p.work_dir, &p.command));
                c
            }
            Shell::PowerShell => {
                let mut c = Command::new("powershell");
                c.raw_arg(ps_launch_args(&p.work_dir, &p.command));
                c
            }
        };
        c.current_dir(&p.work_dir).creation_flags(CREATE_NEW_CONSOLE).spawn()?;
    }
    Ok(())
}
```

注意：`windows.rs` 顶部需要 `use std::os::windows::process::CommandExt;`（`spawn_panes` 里的 `.raw_arg` 以及 `spawn_fallback` 局部 use 均可；同一模块只 import 一次，写在文件顶部）。

- [ ] **Step 6: 更新 platform/mod.rs**

```rust
#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{
    build_wt_commandline, cmd_launch_args, cmd_pane_command, encode_ps_command, fold_cmd_lines,
    ps_launch_args, ps_pane_script, resolve_wt_path, resolve_wt_with, spawn_panes, LaunchMode,
    PaneSpec,
};

#[cfg(not(windows))]
pub struct PaneSpec {
    pub title: String,
    pub work_dir: std::path::PathBuf,
    pub shell: crate::config::Shell,
    pub command: String,
}

#[cfg(not(windows))]
pub enum LaunchMode {
    WindowsTerminal,
    Fallback,
}

#[cfg(not(windows))]
pub fn spawn_panes(_project_name: &str, _panes: &[PaneSpec]) -> std::io::Result<LaunchMode> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "terminal launching is only implemented on Windows",
    ))
}
```

（旧的 `join_lines` / `spawn_script` 导出保留到 Task 2。）

- [ ] **Step 7: 全量测试 + 提交**

Run: `cargo test` → 全绿（旧测试 + 新增）
Run: `npm run build` → 绿（前端未动）

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/config.rs src-tauri/src/platform
git commit -m "feat(platform): pane command builders, wt resolution, one-shot wt spawn"
```

---

## Task 2: config v3 + 迁移 + launcher/commands 切换 + 删除旧机制（原子任务）

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/launcher.rs`
- Delete: `src-tauri/src/ready.rs`
- Modify: `src-tauri/src/platform/windows.rs`（删旧函数）
- Modify: `src-tauri/src/platform/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`（删 `encoding_rs`）

**Interfaces:**
- Consumes: Task 1 的 `Shell` / `PaneSpec` / `spawn_panes` / `LaunchMode`
- Produces:
  - `config::{Item, Project, Settings, AppConfig, ProjectTemplate, parse_config, parse_template}`
  - `launcher::{launch_project, launch_item, launch_items, build_panes, resolve_work_dir, notify}`
  - IPC：`launch_project_cmd(projectId)`、`launch_item_cmd(projectId, itemId)`、`export_project_file(projectId) -> path`

- [ ] **Step 1: 写迁移失败测试（red）**

替换 `config.rs` 测试模块中旧的 group/step 测试，加入：

```rust
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
    assert_eq!(cfg.version, 3);
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
    assert_eq!(t.version, 3);
    assert_eq!(t.items.len(), 1);
    assert_eq!(t.items[0].work_dir.as_deref(), Some("server"));
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"items\""));
    assert!(!json.contains("groups"));
    assert!(!json.contains("readyCondition"));
}
```

Run: `cargo test` → FAIL（模型未实现）

- [ ] **Step 2: 实现 config v3 模型与迁移**

重写 `config.rs`（上方文件审计中的“保留项”逐字保留）。核心代码：

```rust
pub const CONFIG_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
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
    pub items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub autostart: bool,
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

impl AppConfig {
    pub fn new() -> Self { Self::default() }
    pub fn load(path: &Path) -> AppConfig {
        match fs::read_to_string(path) {
            Ok(text) => match parse_config(&text) {
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
    pub fn save(&self, path: &Path) -> Result<(), String> { save_json(self, path) }
}

/// 版本探测后选择 v3 直接解析或 legacy 迁移（v1/v2）。
pub fn parse_config(text: &str) -> Result<AppConfig, serde_json::Error> {
    #[derive(Deserialize)]
    struct VersionProbe {
        #[serde(default)]
        version: u32,
    }
    let probe: VersionProbe = serde_json::from_str(text)?;
    if probe.version >= CONFIG_VERSION {
        serde_json::from_str(text)
    } else {
        let legacy: LegacyAppConfig = serde_json::from_str(text)?;
        Ok(legacy.into_v3())
    }
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
    fn into_v3(self) -> AppConfig {
        AppConfig {
            version: CONFIG_VERSION,
            settings: Settings { autostart: self.settings.autostart },
            projects: self.projects.into_iter().map(LegacyProject::into_v3).collect(),
        }
    }
}

impl LegacyProject {
    fn into_v3(self) -> Project {
        Project {
            id: self.id,
            name: self.name,
            root_dir: self.root_dir,
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
    if parts.is_empty() { ".".into() } else { parts.join("\\") }
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
        Self { version: CONFIG_VERSION, name: p.name.clone(), items: p.items.clone() }
    }
    pub fn load(path: &Path) -> Result<ProjectTemplate, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("读取失败：{e}"))?;
        parse_template(&text).map_err(|e| format!("配置文件格式错误：{e}"))
    }
}

pub fn parse_template(text: &str) -> Result<ProjectTemplate, serde_json::Error> {
    #[derive(Deserialize)]
    struct VersionProbe {
        #[serde(default)]
        version: u32,
    }
    let probe: VersionProbe = serde_json::from_str(text)?;
    if probe.version >= CONFIG_VERSION {
        serde_json::from_str(text)
    } else {
        let legacy: LegacyTemplate = serde_json::from_str(text)?;
        Ok(legacy.into_v3())
    }
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
    fn into_v3(self) -> ProjectTemplate {
        ProjectTemplate {
            version: CONFIG_VERSION,
            name: self.name,
            items: self.groups.into_iter().filter_map(LegacyGroup::into_item).collect(),
        }
    }
}
```

保留测试：`load_missing_file_returns_default`、`save_then_load_roundtrip`（改为 `cfg.settings.autostart = true` 并断言 `loaded.settings.autostart`）、`corrupt_file_backed_up_and_defaulted`、`project_template_strips_root_dir`、`project_template_roundtrip`。删除所有 `Group` / `Step` / `ReadyCondition` / `Terminal` / `migrate_v1` 相关测试与代码。测试模块统一使用新样例：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_project() -> Project {
        Project {
            id: "p1".into(),
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
}
```

- [ ] **Step 3: 重写 launcher.rs**

```rust
use crate::config::{AppConfig, Item, Project};
use crate::platform::{self, LaunchMode, PaneSpec};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn notify(app: &AppHandle, body: String) {
    let _ = app.notification().builder().title("DevLaunch").body(&body).show();
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
    let panes = build_panes(project, items).map_err(|e| { notify(app, e.clone()); e })?;
    let mode = platform::spawn_panes(&project.name, &panes)
        .map_err(|e| { let m = format!("启动失败：{e}"); notify(app, m.clone()); m })?;
    if mode == LaunchMode::Fallback {
        notify(app, "未检测到 Windows Terminal，已降级为独立终端窗口".into());
    }
    notify(app, format!("已启动「{}」（{} 个终端）", project.name, panes.len()));
    Ok(())
}

pub fn build_panes(project: &Project, items: &[Item]) -> Result<Vec<PaneSpec>, String> {
    if items.is_empty() {
        return Err("没有可启动的启动项".into());
    }
    let mut panes = Vec::with_capacity(items.len());
    for item in items {
        let wd = resolve_work_dir(&project.root_dir, &item.work_dir);
        if !wd.is_dir() {
            return Err(format!("「{}」目录不存在：{}", item.name, wd.display()));
        }
        panes.push(PaneSpec {
            title: item.name.clone(),
            work_dir: wd,
            shell: item.shell,
            command: item.command.clone(),
        });
    }
    Ok(panes)
}
```

单测：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Shell;

    fn project() -> Project {
        Project {
            id: "p1".into(), name: "X".into(), root_dir: r"D:\proj".into(),
            items: vec![Item { id: "i1".into(), name: "后端".into(), work_dir: Some("backend".into()), shell: Shell::Cmd, command: "python app.py".into() }],
        }
    }

    #[test]
    fn build_panes_resolves_workdir_and_maps_fields() {
        let p = project();
        let panes = build_panes(&p, &p.items).unwrap();
        assert_eq!(panes[0].work_dir, PathBuf::from(r"D:\proj\backend"));
        assert_eq!(panes[0].title, "后端");
    }

    #[test]
    fn build_panes_errors_on_missing_dir() {
        let mut p = project();
        p.items[0].work_dir = Some("nope-xyz".into());
        assert!(build_panes(&p, &p.items).unwrap_err().contains("目录不存在"));
    }
}
```

- [ ] **Step 4: 删除旧 platform 函数**

`windows.rs` 删除：`join_lines`、`cmd_script_args`、`ps_script_args`、`wt_script_args`、`spawn_script` 及其单测。`platform/mod.rs` 删除对应导出与非 Windows 的 `join_lines` / `spawn_script` stub（Task 1 已加新 stub；不补的新函数不在 mod 导出，则 `commands`/`launcher` 只使用 `spawn_panes` 即可——确保 mod.rs 只导出 launcher 需要的内容）。

- [ ] **Step 5: 删除 ready.rs 与 lib.rs 更新**

- `git rm src-tauri/src/ready.rs`
- `lib.rs`：删除 `pub mod ready;`，更新 `invoke_handler`：

```rust
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::list_subdirs,
            commands::launch_project_cmd,
            commands::launch_item_cmd,
            commands::open_dir,
            commands::export_config_to,
            commands::import_config_from,
            commands::export_project,
            commands::export_project_file,
            commands::read_project_template,
            commands::get_autostart,
            commands::set_autostart,
        ])
```

- [ ] **Step 6: commands.rs 切换**

```rust
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
```

删除 `launch_group_cmd`、`run_step`；删除 `use tauri::{AppHandle, Emitter, State}` 中的 `Emitter`（不再 emit）。`import_config_from` 改为：

```rust
    let text = fs::read_to_string(&path).map_err(|e| format!("读取失败：{e}"))?;
    let cfg = crate::config::parse_config(&text).map_err(|e| format!("配置文件格式错误：{e}"))?;
```

测试模块：`sample_project` 改为 items 版；保留 `subdirs_*`、`export_project_writes_template_without_root_dir`（断言 `"items"`）、`export_project_errors_on_missing_project`、`read_template_rejects_bad_json`；新增：

```rust
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
```

（`export_project_file` 本身含 State，不便单测；测导出路径与内容逻辑。）

- [ ] **Step 7: Cargo.toml 删 encoding_rs，全量测试**

```toml
# 删除 encoding_rs 行
```

Run: `cargo test` → 全绿（旧测试已随代码删除，新测试通过）

- [ ] **Step 8: 前端最小适配以免 `npm run build` 断裂？**

不需要：Task 2 只动 Rust，`npm run build` 仍绿（前端类型未引用 Rust 类型）。继续。

- [ ] **Step 9: 提交**

```bash
git add -A
git commit -m "feat: v3 minimal model - items, legacy migration, one-shot wt launch"
```

---

## Task 3: 前端 v3（启动项编辑器）

**Files:**
- Modify: `src/types.ts`、`src/api.ts`、`src/App.vue`、`src/views/HomeView.vue`、`src/views/EditorView.vue`、`src/views/SettingsView.vue`、`src/style.css`

**Interfaces:**
- Consumes: IPC `launch_project_cmd` / `launch_item_cmd` / `export_project_file` / `read_project_template`（Task 2）
- Produces: 可用的 v3 UI

- [ ] **Step 1: 重写 types.ts**

```ts
export type View = { name: 'home' } | { name: 'editor'; projectId: string } | { name: 'settings' }

export type Shell = 'cmd' | 'powershell'

export interface Item {
  id: string
  name: string
  workDir?: string | null
  shell: Shell
  command: string
}

export interface Project {
  id: string
  name: string
  rootDir: string
  items: Item[]
}

export interface Settings {
  autostart: boolean
}

export interface AppConfig {
  version: number
  settings: Settings
  projects: Project[]
}

export interface ProjectTemplate {
  version: number
  name: string
  items: Item[]
}

export type ToastKind = 'ok' | 'err'

export function newId(): string {
  return crypto.randomUUID()
}

export function newItem(): Item {
  return { id: newId(), name: '', workDir: '', shell: 'cmd', command: '' }
}
```

- [ ] **Step 2: api.ts 更新**

```ts
import { invoke } from '@tauri-apps/api/core'
import type { AppConfig, ProjectTemplate } from './types'

export const getConfig = () => invoke<AppConfig>('get_config')
export const saveConfig = (config: AppConfig) => invoke<void>('save_config', { config })
export const launchProject = (projectId: string) => invoke<void>('launch_project_cmd', { projectId })
export const launchItem = (projectId: string, itemId: string) =>
  invoke<void>('launch_item_cmd', { projectId, itemId })
export const openDir = (path: string) => invoke<void>('open_dir', { path })
export const listSubdirs = (path: string) => invoke<string[]>('list_subdirs', { path })
export const exportConfigTo = (path: string) => invoke<void>('export_config_to', { path })
export const importConfigFrom = (path: string) => invoke<void>('import_config_from', { path })
export const exportProject = (projectId: string, path: string) =>
  invoke<void>('export_project', { projectId, path })
export const exportProjectFile = (projectId: string) =>
  invoke<string>('export_project_file', { projectId })
export const readProjectTemplate = (path: string) =>
  invoke<ProjectTemplate>('read_project_template', { path })
export const getAutostart = () => invoke<boolean>('get_autostart')
export const setAutostart = (enabled: boolean) => invoke<void>('set_autostart', { enabled })
```

- [ ] **Step 3: App.vue 移除 launch-result 监听**

```ts
onMounted(async () => {
  await load()
})
```

（删除 `import { listen }` 与监听块；`launch-result` 事件不再存在。）

- [ ] **Step 4: HomeView.vue 渲染启动项**

Script 部分替换：

```ts
import { computed, ref } from 'vue'
import { config, persist } from '../store'
import { newId, type Item, type Project } from '../types'
import { launchProject, openDir } from '../api'
// removeProject / launch / open 保持不变
function itemLabel(i: Item): string {
  const cmd = i.command.trim().split('\n')[0]?.trim().split(/\s+/)[0] || ''
  return (i.name || cmd || '未命名').trim()
}
```

Template 的 pipeline 块替换为：

```html
          <div class="pc-pipeline">
            <template v-for="(it, i) in p.items" :key="it.id">
              <span v-if="i > 0" class="pl-sep">·</span>
              <span class="pl-cmd" :title="it.command">{{ itemLabel(it) }}</span>
            </template>
            <span v-if="p.items.length === 0" class="pl-empty">无启动项</span>
          </div>
```

`createProject` 改为：

```ts
  config.value.projects.push({ id: newId(), name: '新项目', rootDir: '', items: [] })
```

- [ ] **Step 5: 重写 EditorView.vue**

完整替换 `<script setup>` 与 `<template>`（保留 `.editor/.group/.step-*` 等现有 class 以减少 CSS 改动）：

```vue
<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { config, persist } from '../store'
import { exportProjectFile, launchItem, listSubdirs, readProjectTemplate } from '../api'
import { newItem, type Item } from '../types'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ back: []; notify: [msg: string, kind?: 'ok' | 'err'] }>()

const project = computed(() => config.value!.projects.find((p) => p.id === props.projectId)!)
const savedSnapshot = ref(JSON.stringify(project.value))
const dirty = computed(() => JSON.stringify(project.value) !== savedSnapshot.value)

async function save() {
  try {
    await persist()
    savedSnapshot.value = JSON.stringify(project.value)
    emit('notify', '已保存')
  } catch (e) {
    emit('notify', `保存失败：${e}`, 'err')
  }
}

function addItem() {
  project.value.items.push(newItem())
}

function removeItem(ii: number) {
  project.value.items.splice(ii, 1)
}

function moveItem(ii: number, dir: number) {
  const j = ii + dir
  if (j < 0 || j >= project.value.items.length) return
  ;[project.value.items[ii], project.value.items[j]] = [project.value.items[j], project.value.items[ii]]
}

const subdirs = ref<string[]>([])
const openMenuFor = ref('')

async function refreshSubdirs() {
  const root = project.value?.rootDir
  if (!root) { subdirs.value = []; return }
  try { subdirs.value = await listSubdirs(root) } catch { subdirs.value = [] }
}
watch(() => project.value?.rootDir, () => refreshSubdirs())

function openCombo(itemId: string) {
  openMenuFor.value = itemId
  refreshSubdirs()
}
function closeCombo() { openMenuFor.value = '' }
function pickWorkDir(it: Item, dir: string) {
  it.workDir = dir
  closeCombo()
}

async function tryRunItem(it: Item) {
  try {
    await launchItem(project.value.id, it.id)
    emit('notify', `已启动「${it.name || '启动项'}」`)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function browseRoot() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ directory: true, multiple: false })
  if (typeof picked !== 'string') return
  project.value.rootDir = picked
  await tryAutoImport()
}

function projectFilePath() {
  const root = project.value.rootDir.replace(/[\\/]+$/, '')
  return root ? `${root}\\devlaunch.json` : ''
}

async function tryAutoImport() {
  const path = projectFilePath()
  if (!path || project.value.items.length > 0) return
  try {
    const tpl = await readProjectTemplate(path)
    if (tpl.items.length > 0) {
      project.value.name = tpl.name || project.value.name
      project.value.items = tpl.items
      emit('notify', '已从项目根目录 devlaunch.json 导入启动项')
    }
  } catch {
    // 没有项目文件时静默
  }
}

async function doExportToRoot() {
  const path = projectFilePath()
  if (!path) { emit('notify', '请先设置项目根目录', 'err'); return }
  try {
    await exportProjectFile(project.value.id)
    emit('notify', `已导出到 ${path}`)
  } catch (e) {
    emit('notify', `导出失败：${e}`, 'err')
  }
}

async function doImport() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] })
  if (typeof picked !== 'string') return
  try {
    const tpl = await readProjectTemplate(picked)
    project.value.name = tpl.name || project.value.name
    project.value.items = tpl.items
    await persist()
    savedSnapshot.value = JSON.stringify(project.value)
    emit('notify', '项目配置已导入')
  } catch (e) {
    emit('notify', `导入失败：${e}`, 'err')
  }
}
</script>

<template>
  <div class="editor" v-if="project">
    <div class="editor-head">
      <button class="ghost" @click="emit('back')">← 返回</button>
      <span v-if="dirty" class="dirty-dot" title="有未保存的更改" />
      <input v-model="project.name" class="editor-title grow" placeholder="项目名称" />
      <button class="ghost" title="从项目配置文件导入（覆盖启动项，保留根目录）" @click="doImport">导入</button>
      <button class="ghost" title="导出为项目根目录下的 devlaunch.json" @click="doExportToRoot">导出到项目根</button>
      <button class="primary" @click="save">保存</button>
    </div>

    <div class="pathbar mono">
      <span class="pb-label">ROOT</span>
      <input class="inline" v-model="project.rootDir" placeholder="D:\Projects\my-app" />
      <button class="ghost" @click="browseRoot">选择…</button>
    </div>

    <div v-for="(it, ii) in project.items" :key="it.id" class="group">
      <div class="group-head">
        <span class="group-index">{{ String(ii + 1).padStart(2, '0') }}</span>
        <input v-model="it.name" class="group-name grow" placeholder="启动项名称（如 后端）" />
        <select v-model="it.shell" title="高级：命令方言（默认 CMD）">
          <option value="cmd">CMD</option>
          <option value="powershell">PowerShell</option>
        </select>
        <button class="accent" @click="tryRunItem(it)">▶ 运行此项</button>
        <button class="danger ghost" @click="removeItem(ii)">✕</button>
      </div>

      <div class="steps">
        <div class="step-item">
          <div class="rail"><span class="step-dot">⌘</span></div>
          <div class="step-body">
            <div class="s-main">
              <textarea
                v-model="it.command"
                class="cmd-input"
                rows="3"
                placeholder="按平时手动敲的顺序写，一行一条（如 conda activate xingtu 换行 python -m uvicorn main:app --port 8081）"
                spellcheck="false"
              />
            </div>
            <div class="s-meta">
              <div class="combo">
                <input
                  class="inline"
                  v-model="it.workDir"
                  placeholder="子目录（留空=根目录）"
                  @focus="openCombo(it.id)"
                  @blur="closeCombo"
                  @keydown.esc="closeCombo"
                />
                <div v-if="openMenuFor === it.id" class="combo-menu">
                  <div class="combo-item" :class="{ active: !it.workDir }" @mousedown.prevent="pickWorkDir(it, '')">
                    （根目录）
                  </div>
                  <div
                    v-for="d in subdirs"
                    :key="d"
                    class="combo-item mono"
                    :class="{ active: it.workDir === d }"
                    @mousedown.prevent="pickWorkDir(it, d)"
                  >
                    {{ d }}
                  </div>
                  <div v-if="subdirs.length === 0" class="combo-empty">根目录下没有子目录，可手动输入相对路径</div>
                </div>
              </div>
              <span class="spacer" />
              <button class="ghost" :disabled="ii === 0" title="上移" @click="moveItem(ii, -1)">↑</button>
              <button class="ghost" :disabled="ii === project.items.length - 1" title="下移" @click="moveItem(ii, 1)">↓</button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <button class="ghost" style="width: 100%; border: 1px dashed var(--border-strong)" @click="addItem">
      + 添加启动项
    </button>

    <p class="hint">
      每个启动项 = 一个真实终端窗格：多行命令在同一个 shell 会话里按顺序执行（cd → 激活环境 → 启动服务）。一个项目一键启动 = 一个 Windows Terminal 窗口，每个启动项一个窗格，默认并行启动。需要等待时在命令里自己写等待（CMD：timeout /t 5 /nobreak &gt;nul；PowerShell：Start-Sleep -Seconds 5）。
    </p>
  </div>
</template>
```

- [ ] **Step 6: SettingsView.vue 删除就绪超时**

删除第二个 `list-row`（默认就绪超时）与 `saveTimeout` 函数；保留 autostart 与配置备份。

- [ ] **Step 7: style.css 清理**

删除不再使用的规则（若存在）：`.gate-note`、`.pl-gate`、`.cond-fields`、`.adv-toggle`、`.rail-line`（保留 `.step-dot`）、`.run-step`（不再使用）。若某 class 仍被模板使用则保留。

- [ ] **Step 8: 构建验证 + 提交**

Run: `npm run build` → 绿（vue-tsc + vite）

```bash
git add src
git commit -m "feat(ui): item-based editor, project-file export, remove gates/terminal concepts"
```

---

## Task 4: 文档同步 + 手工冒烟 + 构建验收

**Files:**
- Modify: `docs/PRODUCT.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: 更新 docs/PRODUCT.md**

按 v3 最小架构重写：产品定义（一键重放器）、核心概念（项目 → 启动项）、行为契约（一次 wt 调用一个窗口多窗格、并行启动、Rust 不等待不监控、失败留在终端、降级多窗口）、配置文件（v3 JSON 示例）、界面（启动项列表编辑器）、non-goals（无门控 / 无进程监控 / 无 pane 配置等）、架构速览。将「就绪条件」章节整体删除。

- [ ] **Step 2: 更新 AGENTS.md**

- Architecture 段：`config`（v3 + legacy 迁移）、`platform`（wt 解析 / 命令构造 / 一次 spawn / 降级）、`launcher`（校验+spawn+通知）、`ready.rs` 已删除。
- 硬性约定：删除脚本 / GBK / chcp / call 前缀 / readyCondition 条目；新增「一次 wt 调用一个窗口 N pane」「命令经 raw_arg 传递，cmd 命令整体加引号」「wt 缺失降级多窗口」「CONFIG_VERSION=3」。
- Commands / Testing status 相应更新（测试数量以 `cargo test` 实际为准）。

- [ ] **Step 3: 手工冒烟（人工验收，逐项勾选）**

1. 备份旧配置（`%APPDATA%\com.devlaunch.app\config.json`），用现有 v1 配置启动 → 启动项自动迁移为「后端/前端」，命令正确合并。
2. 新建/编辑 XingTu 项目（rootDir `D:\Projects\XINGTU`）→ 点击卡片：**一个 wt 窗口，2 个窗格**，标题正确，cwd 正确，`conda activate xingtu` 生效，后端启动（DB 告警与手动启动一致）。
3. 每个窗格 `Ctrl+C` → 干净回到提示符（无 `Terminate batch job` 提示）。
4. 再次点击卡片 → 新窗口，旧窗口不受影响。
5. 关闭窗口 → 该窗口服务全部结束。
6. 单启：编辑器中「运行此项」→ 独立窗口启动单个启动项。
7. 降级：`set DEVLAUNCH_WT_PATH=` 后重启 DevLaunch 再启动 → 独立终端窗口 + 降级通知。
8. 中文 / emoji / 带空格路径 / 带引号命令冒烟（至少 2 条复杂命令）。
9. 导入导出：导出到项目根生成 `devlaunch.json`（version 3、无 rootDir）；新项目选择该目录 → 自动导入提示。

- [ ] **Step 4: 构建验收**

Run: `npm run build` → 绿
Run: `cargo test`（在 `src-tauri/` 下）→ 全绿
Run: `npm run tauri build` → 产物 `src-tauri/target/release/devlaunch.exe` + bundle（一次）

- [ ] **Step 5: 提交**

```bash
git add docs AGENTS.md
git commit -m "docs: sync PRODUCT.md and AGENTS.md to v3 minimal architecture"
```

---

## 已知风险与验证点

1. **`wt → cmd /K` 引号传递**（唯一高风险项）：Task 1 的单测只锁定字符串构造，不能证明 wt 的实际解析行为。Task 4 Step 3 冒烟第 2 / 8 条必须重点验证含引号、空格路径、`&&`、中文的命令。若失败：按 spec §4 兜底预案改为「每项一行临时脚本 `cmd /K call "%TEMP%\devlaunch-item-<id>.cmd"`」，仅改 `platform` 层 cmd 构造，用户模型不变。
2. **wt 窗口被关卡后补 pane**：本方案一次调用建完所有 pane，不存在该问题（相对旧 v3 的直接收益）。
3. **超长命令行**（32767）：已实现为 Task 1 的 `validate_wt_commandline`（>30000 字符报错提示拆分启动项），单测覆盖。

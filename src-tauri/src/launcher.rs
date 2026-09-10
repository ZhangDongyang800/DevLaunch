use crate::config::{AppConfig, Group, Project, Terminal};
use crate::platform;
use crate::ready;
use std::fs;
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
    let windows = project.groups.iter().filter(|g| !g.steps.is_empty()).count();
    for group in &project.groups {
        launch_group_steps(app, project, group, cfg.settings.ready_timeout_sec)?;
    }
    notify(app, format!("已启动「{}」（{} 个终端窗口）", project.name, windows));
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
    // 单步运行 = 单步临时脚本，终端沿用分组终端
    let single = Group {
        id: format!("step-{}", step.id),
        name: group.name.clone(),
        terminal: group.terminal,
        steps: vec![step.clone()],
    };
    launch_group_steps(app, project, &single, cfg.settings.ready_timeout_sec)
}

/// v2 编排：一个分组 = 一个终端窗口。把组内全部步骤（含就绪等待）生成一个
/// 临时脚本，一次 spawn；Rust 不再同步等待，就绪失败在终端窗口内提示并停住。
pub fn launch_group_steps(app: &AppHandle, project: &Project, group: &Group, default_timeout: u64) -> Result<(), String> {
    if group.steps.is_empty() {
        return Ok(());
    }
    for step in &group.steps {
        let wd = resolve_work_dir(&project.root_dir, &step.work_dir);
        if !wd.is_dir() {
            let msg = format!("「{}」目录不存在：{}", step.name, wd.to_string_lossy());
            notify(app, msg.clone());
            return Err(msg);
        }
    }
    let script = write_group_script(project, group, default_timeout)?;
    platform::spawn_script(group.terminal, &project.root_dir, &script.to_string_lossy()).map_err(|e| {
        let msg = format!("「{}」启动失败：{e}", group.name);
        notify(app, msg.clone());
        msg
    })
}

/// 脚本体：每步「cd（目录变化时）→ 命令（多行按 shell 语义并入）→ 就绪等待块」。
pub fn build_group_script(project: &Project, group: &Group, default_timeout: u64) -> String {
    let ps = matches!(group.terminal, Terminal::PowerShell);
    let step_sep = if ps { "; " } else { " && " };
    let mut lines: Vec<String> = Vec::new();
    let mut current: Option<PathBuf> = None;
    for step in &group.steps {
        let wd = resolve_work_dir(&project.root_dir, &step.work_dir);
        if current.as_deref() != Some(wd.as_path()) {
            lines.push(cd_line(&wd.to_string_lossy(), ps));
            current = Some(wd);
        }
        let joined = platform::join_lines(&step.command, step_sep);
        if !joined.is_empty() {
            // batch 内调用另一个 batch（conda.bat/npm.cmd…）不加 call 会转移控制权且不返回，
            // 吞掉后续步骤；内部命令与 exe 不受 call 影响，故 cmd 方言统一加前缀。
            lines.push(if ps { joined } else { call_prefixed(&joined) });
        }
        let wait = if ps {
            ready::wait_block_ps(&step.ready_condition, default_timeout, &step.name)
        } else {
            ready::wait_block_cmd(&step.ready_condition, default_timeout, &step.name)
        };
        if !wait.is_empty() {
            lines.push(wait);
        }
    }
    lines.join("\r\n")
}

fn cd_line(dir: &str, ps: bool) -> String {
    if ps {
        format!("Set-Location '{}'", dir.replace('\'', "''"))
    } else {
        format!("cd /d \"{dir}\"")
    }
}

fn call_prefixed(command: &str) -> String {
    let trimmed = command.trim_start();
    if trimmed.len() >= 5 && trimmed[..5].eq_ignore_ascii_case("call ") {
        command.to_string()
    } else {
        format!("call {command}")
    }
}

fn script_ext(terminal: Terminal) -> &'static str {
    if matches!(terminal, Terminal::PowerShell) { "ps1" } else { "cmd" }
}

/// 脚本路径按组固定命名：多次运行覆盖同一文件，不在 %TEMP% 累积。
pub fn script_path_for_group(kind: &str, group_id: &str, terminal: Terminal) -> PathBuf {
    let safe: String = group_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    std::env::temp_dir().join(format!("devlaunch-{kind}-{safe}.{}", script_ext(terminal)))
}

pub fn write_group_script(project: &Project, group: &Group, default_timeout: u64) -> Result<PathBuf, String> {
    let body = build_group_script(project, group, default_timeout);
    let path = script_path_for_group("group", &group.id, group.terminal);
    write_script_file(&path, &body, group.terminal)?;
    Ok(path)
}

fn write_script_file(path: &Path, body: &str, terminal: Terminal) -> Result<(), String> {
    // cmd 按系统代码页（中文 Windows 为 GBK）逐行解码脚本：直接以 GBK 编码写文件，
    // 中文命令在 GBK 终端里正确显示。不能写 UTF-8：无 BOM 会被当 ANSI 乱码；
    // 也不能 chcp 65001——实测 chcp 65001 会使 conda.bat 激活失败（RC=3）。
    if matches!(terminal, Terminal::PowerShell) {
        // PowerShell 5.1 无 BOM 的 UTF-8 会被当 ANSI：写 UTF-8 BOM
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(body.as_bytes());
        fs::write(path, bytes).map_err(|e| e.to_string())
    } else {
        let (bytes, _, had_errors) = encoding_rs::GBK.encode(body);
        if had_errors {
            // 步骤命令含 GBK 无法编码的字符：退回 UTF-8 + chcp（放弃 conda 兼容保编码）
            fs::write(path, format!("chcp 65001 >nul\r\n{body}")).map_err(|e| e.to_string())
        } else {
            fs::write(path, &bytes).map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ReadyCondition, Step};

    fn step(name: &str, work_dir: Option<&str>, command: &str, cond: ReadyCondition) -> Step {
        Step {
            id: format!("id-{name}"),
            name: name.into(),
            work_dir: work_dir.map(Into::into),
            terminal: Terminal::Cmd,
            command: command.into(),
            ready_condition: cond,
        }
    }

    fn project(group: Group) -> Project {
        Project {
            id: "p1".into(),
            name: "PVDS".into(),
            root_dir: r"D:\proj".into(),
            groups: vec![group],
        }
    }

    #[test]
    fn group_script_joins_steps_and_inserts_wait_blocks() {
        let g = Group {
            id: "g1".into(),
            name: "后端".into(),
            terminal: Terminal::Cmd,
            steps: vec![
                step("env", Some("backend"), "conda activate xingtu", ReadyCondition::Delay { seconds: 5 }),
                step("srv", Some("backend"), "python -m uvicorn\n--reload", ReadyCondition::Immediate),
            ],
        };
        let p = project(g.clone());
        let s = build_group_script(&p, &g, 30);
        assert_eq!(
            s,
            "cd /d \"D:\\proj\\backend\"\r\ncall conda activate xingtu\r\ntimeout /t 5 /nobreak >nul\r\ncall python -m uvicorn && --reload"
        );
    }

    #[test]
    fn cmd_step_already_call_prefixed_not_duplicated() {
        let g = Group {
            id: "g1".into(),
            name: "g".into(),
            terminal: Terminal::Cmd,
            steps: vec![
                step("a", None, "call conda activate x", ReadyCondition::Immediate),
                step("b", None, "CALL setup.bat", ReadyCondition::Immediate),
            ],
        };
        let p = project(g.clone());
        let s = build_group_script(&p, &g, 30);
        assert!(s.contains("call conda activate x"));
        assert!(s.contains("CALL setup.bat"));
        assert!(!s.contains("call call"));
    }

    #[test]
    fn group_script_switches_workdir_per_step() {
        let g = Group {
            id: "g1".into(),
            name: "全栈".into(),
            terminal: Terminal::Cmd,
            steps: vec![
                step("api", Some("server"), "python app.py", ReadyCondition::Port { port: 8000, host: "127.0.0.1".into(), timeout_sec: 30 }),
                step("web", Some("web"), "npm run dev", ReadyCondition::Immediate),
            ],
        };
        let p = project(g.clone());
        let s = build_group_script(&p, &g, 30);
        assert!(s.starts_with("cd /d \"D:\\proj\\server\"\r\ncall python app.py\r\n"));
        assert!(s.contains("if errorlevel 1"));
        assert!(s.ends_with("cd /d \"D:\\proj\\web\"\r\ncall npm run dev"));
    }

    #[test]
    fn group_script_powershell_dialect() {
        let g = Group {
            id: "g1".into(),
            name: "ps".into(),
            terminal: Terminal::PowerShell,
            steps: vec![
                step("env", None, "conda activate x\npython run.py", ReadyCondition::Immediate),
            ],
        };
        let p = project(g.clone());
        let s = build_group_script(&p, &g, 30);
        assert!(s.starts_with("Set-Location 'D:\\proj'\r\n"));
        assert!(s.contains("conda activate x; python run.py"));
    }

    #[test]
    fn script_path_fixed_per_group_and_ext_per_terminal() {
        let cmd = script_path_for_group("group", "abc/123", Terminal::Cmd);
        assert!(cmd.to_string_lossy().ends_with("devlaunch-group-abc-123.cmd"));
        let ps = script_path_for_group("step", "xyz", Terminal::PowerShell);
        assert!(ps.to_string_lossy().ends_with("devlaunch-step-xyz.ps1"));
    }

    #[test]
    fn write_script_cmd_is_gbk_without_chcp() {
        let dir = std::env::temp_dir();
        let path = dir.join("devlaunch-test-write.cmd");
        write_script_file(&path, "call conda activate env\r\necho 中文目录\r\n", Terminal::Cmd).unwrap();
        let bytes = fs::read(&path).unwrap();
        // 无 chcp 头
        assert!(bytes.starts_with(b"call conda activate env"));
        // 中文按 GBK 可解码回原文
        let (text, _, _) = encoding_rs::GBK.decode(&bytes);
        assert!(text.contains("echo 中文目录"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn write_script_cmd_unencodable_falls_back_to_utf8_chcp() {
        let dir = std::env::temp_dir();
        let path = dir.join("devlaunch-test-write2.cmd");
        write_script_file(&path, "echo emoji \u{1F600}\r\n", Terminal::Cmd).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("chcp 65001 >nul\r\n"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn write_script_ps1_has_bom() {
        let dir = std::env::temp_dir();
        let path = dir.join("devlaunch-test-write.ps1");
        write_script_file(&path, "Write-Host hi", Terminal::PowerShell).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        let _ = fs::remove_file(&path);
    }
}

use crate::config::{Shell, Terminal};
use std::ffi::OsStr;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 多行命令合并为单行（同一终端窗口内顺序执行）：去空行、trim，按 shell 语义连接。
pub fn join_lines(command: &str, sep: &str) -> String {
    command
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<&str>>()
        .join(sep)
}

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn cmd_script_args(script_path: &str) -> String {
    format!("/K call \"{script_path}\"")
}

pub fn ps_script_args(script_path: &str) -> String {
    format!("-NoExit -ExecutionPolicy Bypass -File \"{script_path}\"")
}

pub fn wt_script_args(work_dir: &str, script_path: &str) -> String {
    format!("-d \"{work_dir}\" cmd /K call \"{script_path}\"")
}

/// 启动一个终端窗口执行临时脚本文件（组内所有步骤 + 就绪等待都在脚本里）。
/// cmd/wt 走 `cmd /K call`，powershell 走 `-File`；窗口常驻（-NoExit / /K）。
pub fn spawn_script(terminal: Terminal, work_dir: &str, script_path: &str) -> std::io::Result<()> {
    let args = match terminal {
        Terminal::Cmd => cmd_script_args(script_path),
        Terminal::PowerShell => ps_script_args(script_path),
        Terminal::WindowsTerminal => {
            // wt 不加 CREATE_NEW_CONSOLE：由 wt 自身管理窗口
            let mut c = Command::new("wt");
            c.current_dir(work_dir).raw_arg(wt_script_args(work_dir, script_path));
            return c.spawn().map(|_| ());
        }
    };
    let program = match terminal {
        Terminal::PowerShell => "powershell",
        _ => "cmd",
    };
    let mut c = Command::new(program);
    c.current_dir(work_dir).raw_arg(args);
    c.creation_flags(CREATE_NEW_CONSOLE);
    c.spawn().map(|_| ())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Shell;
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    #[test]
    fn join_lines_basic() {
        assert_eq!(join_lines("a\n b \n\nc", " && "), "a && b && c");
        assert_eq!(join_lines("a\nb", "; "), "a; b");
        assert_eq!(join_lines("\n \n", " && "), "");
    }

    #[test]
    fn script_args_per_terminal() {
        assert_eq!(cmd_script_args(r"C:\Temp\a.cmd"), r#"/K call "C:\Temp\a.cmd""#);
        assert_eq!(
            ps_script_args(r"C:\Temp\a.ps1"),
            r#"-NoExit -ExecutionPolicy Bypass -File "C:\Temp\a.ps1""#
        );
        assert_eq!(
            wt_script_args(r"D:\proj", r"C:\Temp\a.cmd"),
            r#"-d "D:\proj" cmd /K call "C:\Temp\a.cmd""#
        );
    }

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
}

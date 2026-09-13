use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GIT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

/// `DEVLAUNCH_GIT_PATH` → PATH 中的 git.exe → `%ProgramFiles%\Git\cmd\git.exe`
/// → `%ProgramFiles%\Git\bin\git.exe`。空串 = 强制缺失（供测试/排障）。
pub fn resolve_git_with(
    override_path: Option<&OsStr>,
    path_env: Option<&OsStr>,
    program_files: Option<&OsStr>,
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
            let cand = dir.join("git.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let root = program_files?;
    for rel in [["Git", "cmd", "git.exe"], ["Git", "bin", "git.exe"]] {
        let cand = PathBuf::from(root).join(rel[0]).join(rel[1]).join(rel[2]);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

pub fn resolve_git_path() -> Option<PathBuf> {
    resolve_git_with(
        std::env::var_os("DEVLAUNCH_GIT_PATH").as_deref(),
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("ProgramFiles").as_deref(),
    )
}

pub fn is_not_repo(stderr: &str) -> bool {
    stderr.contains("not a git repository")
}

pub fn friendly_git_error(stderr: &str) -> String {
    if is_not_repo(stderr) {
        return "不是 git 仓库".into();
    }
    let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("git 执行失败");
    last.trim().to_string()
}

#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

pub(crate) fn run_git_with(program: &Path, dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.arg("-C")
        .arg(dir)
        .arg("--no-pager")
        .arg("-c")
        .arg("color.ui=false")
        .arg("-c")
        .arg("core.quotepath=false")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    hide_console(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| format!("启动 git 失败：{e}"))?;
    let mut out_pipe = child.stdout.take().expect("piped stdout");
    let mut err_pipe = child.stderr.take().expect("piped stderr");
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.by_ref().take(MAX_OUTPUT_BYTES).read_to_end(&mut buf);
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.by_ref().take(64 * 1024).read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    let finished = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= GIT_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };

    let out = out_handle.join().unwrap_or_default();
    let err = err_handle.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err).into_owned();

    match finished {
        Some(status) if status.success() => Ok(String::from_utf8_lossy(&out).into_owned()),
        Some(_) => Err(friendly_git_error(&stderr)),
        None => Err("git 执行超时".into()),
    }
}

pub fn run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let program = resolve_git_path().ok_or_else(|| {
        "未找到 git.exe，请安装 Git for Windows 或设置 DEVLAUNCH_GIT_PATH".to_string()
    })?;
    run_git_with(&program, dir, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_git_prefers_override_file() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("git.exe");
        std::fs::write(&fake, "x").unwrap();
        let got = resolve_git_with(Some(fake.as_os_str()), None, None);
        assert_eq!(got.as_deref(), Some(fake.as_path()));
    }

    #[test]
    fn resolve_git_empty_override_forces_none() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("git.exe");
        std::fs::write(&fake, "x").unwrap();
        assert_eq!(resolve_git_with(Some(OsStr::new("")), Some(dir.path().as_os_str()), None), None);
    }

    #[test]
    fn resolve_git_falls_back_to_program_files_cmd_then_bin() {
        let dir = tempfile::tempdir().unwrap();
        let cmd = dir.path().join("Git").join("cmd").join("git.exe");
        std::fs::create_dir_all(cmd.parent().unwrap()).unwrap();
        std::fs::write(&cmd, "x").unwrap();
        let got = resolve_git_with(None, None, Some(dir.path().as_os_str()));
        assert_eq!(got.as_deref(), Some(cmd.as_path()));
    }

    #[test]
    fn resolve_git_path_scan_finds_git_exe() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("git.exe");
        std::fs::write(&bin, "x").unwrap();
        let joined = std::env::join_paths([dir.path()]).unwrap();
        let got = resolve_git_with(None, Some(joined.as_os_str()), None);
        assert_eq!(got.as_deref(), Some(bin.as_path()));
    }

    #[test]
    fn friendly_error_detects_non_repo() {
        assert!(is_not_repo("fatal: not a git repository (or any of the parent directories): .git"));
        assert_eq!(friendly_git_error("fatal: not a git repository (or any of the parent directories): .git"), "不是 git 仓库");
    }

    #[test]
    fn friendly_error_falls_back_to_last_line() {
        let err = friendly_git_error("warning: x\nfatal: bad revision 'abc'");
        assert_eq!(err, "fatal: bad revision 'abc'");
    }
}

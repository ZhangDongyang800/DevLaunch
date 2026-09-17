use crate::git::{run_git, run_git_opts, GitRunOpts};
use std::path::Path;
use std::time::Duration;

const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn validate_paths(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("没有选择文件".into());
    }
    for p in paths {
        let t = p.trim();
        if t.is_empty() {
            return Err("文件路径为空".into());
        }
        if t.contains('\0') {
            return Err(format!("非法路径：{t}"));
        }
    }
    Ok(())
}

fn run_write(dir: &Path, args: &[&str]) -> Result<String, String> {
    run_git_opts(dir, args, GitRunOpts { allow_prompt: false, timeout: WRITE_TIMEOUT, stdin_data: None })
}

pub fn stage(dir: &Path, paths: &[String]) -> Result<(), String> {
    validate_paths(paths)?;
    let mut args = vec!["add", "--"];
    args.extend(paths.iter().map(String::as_str));
    run_write(dir, &args).map(|_| ())
}

pub fn unstage(dir: &Path, paths: &[String]) -> Result<(), String> {
    validate_paths(paths)?;
    let mut args = vec!["restore", "--staged", "--"];
    args.extend(paths.iter().map(String::as_str));
    run_write(dir, &args).map(|_| ())
}

pub fn discard(dir: &Path, paths: &[String]) -> Result<(), String> {
    validate_paths(paths)?;
    for p in paths {
        if run_git(dir, &["ls-files", "--error-unmatch", "--", p.trim()]).is_err() {
            return Err(format!("未跟踪文件不会被丢弃：{p}"));
        }
    }
    let mut args = vec!["restore", "--"];
    args.extend(paths.iter().map(String::as_str));
    run_write(dir, &args).map(|_| ())
}

pub fn commit(dir: &Path, message: &str, amend: bool) -> Result<String, String> {
    let msg = message.trim();
    if msg.is_empty() && !amend {
        return Err("提交信息不能为空".into());
    }
    let mut args = vec!["commit"];
    if amend {
        args.push("--amend");
    }
    args.push("-F");
    args.push("-");
    run_git_opts(
        dir,
        &args,
        GitRunOpts { allow_prompt: false, timeout: WRITE_TIMEOUT, stdin_data: Some(msg.as_bytes().to_vec()) },
    )?;
    Ok(run_git(dir, &["rev-parse", "HEAD"])?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let program = crate::git::resolve_git_path().expect("git installed");
        let out = Command::new(program).arg("-C").arg(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "t@e.com"]);
        git(dir.path(), &["config", "user.name", "T"]);
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn validate_paths_rejects_empty() {
        assert!(validate_paths(&[]).is_err());
        assert!(validate_paths(&["".into()]).is_err());
        assert!(validate_paths(&["a.txt".into()]).is_ok());
    }

    #[test]
    fn stage_unstage_commit_and_discard() {
        if crate::git::resolve_git_path().is_none() {
            return;
        }
        let repo = init_repo();
        let dir = repo.path();

        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        stage(dir, &["a.txt".into()]).unwrap();
        assert!(crate::git::repo_status("p", dir).staged >= 1);
        unstage(dir, &["a.txt".into()]).unwrap();
        assert_eq!(crate::git::repo_status("p", dir).staged, 0);

        stage(dir, &["a.txt".into()]).unwrap();
        let hash = commit(dir, "second", false).unwrap();
        assert_eq!(hash.len(), 40);
        assert_eq!(crate::git::git_log(dir, 10, 0).unwrap().len(), 2);

        std::fs::write(dir.join("new.txt"), "x").unwrap();
        assert!(discard(dir, &["new.txt".into()]).unwrap_err().contains("未跟踪"));

        std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
        discard(dir, &["a.txt".into()]).unwrap();
        let content = std::fs::read_to_string(dir.join("a.txt")).unwrap_or_default();
        assert_eq!(content.replace("\r\n", "\n"), "one\ntwo\n");
    }
}

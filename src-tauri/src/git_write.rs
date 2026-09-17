use crate::git::{run_git, run_git_opts, validate_hash, GitRunOpts};
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

/// 分支名校验（git check-ref-format 的核心规则，拒绝 `--option` 注入）。
pub fn validate_ref_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("分支名为空".into());
    }
    let bad = n.starts_with('-')
        || n.starts_with('.')
        || n.ends_with('.')
        || n.ends_with(".lock")
        || n.contains("..")
        || n.contains("@{")
        || n.chars().any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c));
    if bad {
        return Err(format!("非法分支名：{n}"));
    }
    Ok(())
}

/// merge/rebase 失败后若仓库进入冲突态，返回可读的中文提示。
fn conflict_or(dir: &Path, err: String) -> String {
    if crate::git::in_progress(dir).is_some() {
        "操作产生冲突，请在终端解决后再回到这里（未完成的合并/rebase 会在这里提示）".into()
    } else {
        err
    }
}

pub fn switch_branch(dir: &Path, name: &str) -> Result<(), String> {
    validate_ref_name(name)?;
    run_write(dir, &["switch", name.trim()]).map(|_| ())
}

pub fn create_branch(dir: &Path, name: &str, checkout: bool) -> Result<(), String> {
    validate_ref_name(name)?;
    let args = if checkout { vec!["switch", "-c", name.trim()] } else { vec!["branch", name.trim()] };
    run_write(dir, &args).map(|_| ())
}

pub fn delete_branch(dir: &Path, name: &str, force: bool) -> Result<(), String> {
    validate_ref_name(name)?;
    let flag = if force { "-D" } else { "-d" };
    run_write(dir, &["branch", flag, name.trim()]).map(|_| ())
}

pub fn rename_branch(dir: &Path, old: &str, new: &str) -> Result<(), String> {
    validate_ref_name(old)?;
    validate_ref_name(new)?;
    run_write(dir, &["branch", "-m", old.trim(), new.trim()]).map(|_| ())
}

pub fn merge(dir: &Path, name: &str) -> Result<(), String> {
    validate_ref_name(name)?;
    run_write(dir, &["merge", name.trim()]).map(|_| ()).map_err(|e| conflict_or(dir, e))
}

pub fn rebase(dir: &Path, onto: &str) -> Result<(), String> {
    validate_ref_name(onto)?;
    run_write(dir, &["rebase", onto.trim()]).map(|_| ()).map_err(|e| conflict_or(dir, e))
}

pub fn revert(dir: &Path, hash: &str) -> Result<(), String> {
    validate_hash(hash)?;
    run_write(dir, &["revert", "--no-edit", hash.trim()]).map(|_| ()).map_err(|e| conflict_or(dir, e))
}

pub fn cherry_pick(dir: &Path, hash: &str) -> Result<(), String> {
    validate_hash(hash)?;
    run_write(dir, &["cherry-pick", hash.trim()]).map(|_| ()).map_err(|e| conflict_or(dir, e))
}

/// 仅支持 soft / mixed（绝不 hard）。
pub fn reset(dir: &Path, hash: &str, mode: &str) -> Result<(), String> {
    validate_hash(hash)?;
    let flag = match mode {
        "soft" => "--soft",
        "mixed" => "--mixed",
        _ => return Err("不支持的 reset 模式（仅 soft / mixed）".into()),
    };
    run_write(dir, &["reset", flag, hash.trim()]).map(|_| ())
}

const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// 网络操作：允许 Git Credential Manager 弹窗，长超时。
fn run_net(dir: &Path, args: &[&str]) -> Result<String, String> {
    run_git_opts(dir, args, GitRunOpts { allow_prompt: true, timeout: NETWORK_TIMEOUT, stdin_data: None })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushOutcome {
    pub branch: String,
    pub set_upstream: bool,
}

pub fn fetch(dir: &Path) -> Result<(), String> {
    run_net(dir, &["fetch"]).map(|_| ())
}

pub fn pull(dir: &Path) -> Result<(), String> {
    run_net(dir, &["pull"]).map(|_| ()).map_err(|e| conflict_or(dir, e))
}

pub fn push(dir: &Path) -> Result<PushOutcome, String> {
    let branch = run_git(dir, &["symbolic-ref", "--short", "HEAD"])
        .map_err(|_| "当前处于 detached HEAD，无法推送".to_string())?
        .trim()
        .to_string();
    match run_net(dir, &["push"]) {
        Ok(_) => Ok(PushOutcome { branch, set_upstream: false }),
        Err(e) => {
            let no_upstream = e.contains("no upstream branch")
                || e.contains("has no upstream branch")
                || e.contains("set-upstream");
            if no_upstream && run_git(dir, &["remote", "get-url", "origin"]).is_ok() {
                run_net(dir, &["push", "-u", "origin", branch.as_str()])?;
                Ok(PushOutcome { branch, set_upstream: true })
            } else {
                Err(e)
            }
        }
    }
}

/// `.git/FETCH_HEAD` 的 mtime（Unix 秒）；从未 fetch 过返回 None。
pub fn last_fetch(dir: &Path) -> Option<u64> {
    let git_dir = run_git(dir, &["rev-parse", "--absolute-git-dir"]).ok()?;
    let path = std::path::PathBuf::from(git_dir.trim()).join("FETCH_HEAD");
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
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
    fn validate_ref_name_matrix() {        assert!(validate_ref_name("feature/x").is_ok());
        assert!(validate_ref_name("main").is_ok());
        assert!(validate_ref_name("").is_err());
        assert!(validate_ref_name("-D").is_err());
        assert!(validate_ref_name("a b").is_err());
        assert!(validate_ref_name("a..b").is_err());
        assert!(validate_ref_name("a.lock").is_err());
        assert!(validate_ref_name("a@{b").is_err());
    }

    #[test]
    fn reset_rejects_hard_and_bad_hash() {
        let dir = tempfile::tempdir().unwrap();
        assert!(reset(dir.path(), "aaaa", "hard").unwrap_err().contains("不支持"));
        assert!(reset(dir.path(), "--all", "soft").is_err());
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

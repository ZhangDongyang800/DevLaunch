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

    /// 写一个 pre-commit 钩子（Git for Windows 用自带的 sh 执行）。
    fn write_hook(dir: &Path, script: &str) {
        let hooks = dir.join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        std::fs::write(hooks.join("pre-commit"), format!("#!/bin/sh\n{script}\nexit 0\n")).unwrap();
    }

    #[test]
    fn commit_with_huge_hook_output_and_stdin_does_not_deadlock() {
        if crate::git::resolve_git_path().is_none() {
            return;
        }
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("a.txt"), "churn\n").unwrap();
        crate::git::run_git(dir, &["add", "a.txt"]).unwrap();
        // 远超 64KB 管道缓冲区的钩子输出 + 超过管道缓冲的 stdin 提交信息：
        // 读线程必须先于写 stdin 启动，否则两边互等。
        write_hook(
            dir,
            "i=0\nwhile [ $i -lt 2000 ]; do echo \"chatter $i ---------------------------------------------\"; echo \"noise $i ---------------------------------------------\" >&2; i=$((i+1)); done",
        );
        let msg = format!("chatty hook\n{}", "x".repeat(70 * 1024));
        let start = std::time::Instant::now();
        let hash = commit(dir, &msg, false).expect("commit must not deadlock");
        assert_eq!(hash.trim().len(), 40);
        assert!(start.elapsed() < Duration::from_secs(60), "commit took {:?}", start.elapsed());
        assert_eq!(crate::git::git_log(dir, 5, 0).unwrap().len(), 2);
        let body = crate::git::run_git(dir, &["log", "-1", "--format=%s"]).unwrap();
        assert!(body.starts_with("chatty hook"), "message lost: {body}");
    }

    #[test]
    fn empty_commit_error_comes_from_stdout_not_hook_noise() {
        if crate::git::resolve_git_path().is_none() {
            return;
        }
        let repo = init_repo();
        let dir = repo.path();
        write_hook(dir, "echo hook-noise-line");
        // 没有暂存改动：git 把结论写在 stdout，stderr 只有钩子噪音。
        let err = commit(dir, "nothing staged", false).unwrap_err();
        assert!(!err.contains("hook-noise-line"), "{err}");
        assert!(err.contains("no changes added") || err.contains("nothing to commit"), "{err}");
    }

    #[test]
    fn timed_out_git_kills_hook_tree_and_stale_index_lock() {
        if crate::git::resolve_git_path().is_none() {
            return;
        }
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("a.txt"), "hang\n").unwrap();
        crate::git::run_git(dir, &["add", "a.txt"]).unwrap();
        // 后台子进程在 4 秒后留下标记文件：如果强杀只杀掉 git.exe，它就会活下来。
        write_hook(
            dir,
            "(sleep 4 && echo late > .git/late-marker) &\n: > .git/index.lock\nsleep 120",
        );

        let start = std::time::Instant::now();
        let err = crate::git::run_git_opts(
            dir,
            &["commit", "-m", "hangs"],
            GitRunOpts { allow_prompt: false, timeout: Duration::from_secs(2), stdin_data: None },
        )
        .expect_err("hook hangs, so the run must be interrupted");
        assert!(err.contains("超时"), "{err}");
        // 只杀 git.exe 的话，sh/sleep 还活着并握着管道，这里会等满 120s。
        assert!(start.elapsed() < Duration::from_secs(25), "took {:?}", start.elapsed());
        // 强杀留下的 index.lock 必须被清掉，否则之后所有写操作永久失败。
        assert!(!dir.join(".git").join("index.lock").exists(), "index.lock left behind");
        // 等到后台标记本该出现的时间点之后，确认进程树里没留下任何活口。
        while start.elapsed() < Duration::from_secs(7) {
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(!dir.join(".git").join("late-marker").exists(), "钩子的孙进程未被清理");
        // 仓库仍可写：证明没有残留锁或僵尸进程占住。
        stage(dir, &["a.txt".into()]).expect("stage after the killed commit");
    }

    #[test]
    fn fetch_and_push_work_against_local_remote() {
        if crate::git::resolve_git_path().is_none() {
            return;
        }
        let upstream = init_repo();
        // 推送到对端已检出的分支需要放开默认保护
        git(upstream.path(), &["config", "receive.denyCurrentBranch", "ignore"]);
        let parent = tempfile::tempdir().unwrap();
        git(parent.path(), &["clone", "--quiet", &upstream.path().to_string_lossy(), "work"]);
        let work = parent.path().join("work");
        git(&work, &["config", "user.email", "t@e.com"]);
        git(&work, &["config", "user.name", "T"]);

        std::fs::write(work.join("b.txt"), "new\n").unwrap();
        crate::git::run_git(&work, &["add", "b.txt"]).unwrap();
        commit(&work, "from clone", false).unwrap();
        let pushed = push(&work).expect("push 到已有 upstream");
        assert!(!pushed.set_upstream, "{pushed:?}");

        // 新分支没有 upstream → 自动 -u origin <branch>
        crate::git::run_git(&work, &["switch", "-c", "feature"]).unwrap();
        let second = push(&work).expect("push 自动设置 upstream");
        assert_eq!(second.branch, "feature");
        assert!(second.set_upstream, "{second:?}");

        // 对端从克隆库往回 fetch（origin 指回 work）
        git(upstream.path(), &["remote", "add", "origin", &work.to_string_lossy()]);
        fetch(upstream.path()).expect("fetch");
        assert!(last_fetch(upstream.path()).is_some(), "FETCH_HEAD mtime missing");
        assert!(crate::git::run_git(upstream.path(), &["rev-parse", "--verify", "refs/remotes/origin/feature"]).is_ok());
        // 推送本身也已经落到对端的本地分支
        crate::git::run_git(upstream.path(), &["rev-parse", "--verify", "feature"])
            .expect("feature 应当已经到对端");
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

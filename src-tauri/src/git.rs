use serde::Serialize;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub index: char,
    pub worktree: char,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub project_id: String,
    pub is_repo: bool,
    pub branch: Option<String>,
    pub detached: bool,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub files: Vec<FileChange>,
    pub error: Option<String>,
}

impl RepoStatus {
    pub fn not_repo(project_id: &str) -> Self {
        Self {
            project_id: project_id.into(),
            is_repo: false,
            branch: None,
            detached: false,
            ahead: 0,
            behind: 0,
            staged: 0,
            unstaged: 0,
            untracked: 0,
            files: Vec::new(),
            error: None,
        }
    }

    pub fn errored(project_id: &str, message: String) -> Self {
        let mut s = Self::not_repo(project_id);
        s.error = Some(message);
        s
    }
}

pub fn status_label(index: char, worktree: char) -> String {
    match (index, worktree) {
        ('?', _) => "未跟踪",
        ('!', _) => "忽略",
        ('U', _) | (_, 'U') => "冲突",
        ('D', _) | (_, 'D') => "删除",
        ('A', _) => "新增",
        ('R', _) => "重命名",
        ('C', _) => "复制",
        ('M', _) | (_, 'M') => "修改",
        _ => "变更",
    }
    .to_string()
}

pub fn parse_status(project_id: &str, text: &str) -> RepoStatus {
    let mut st = RepoStatus::not_repo(project_id);
    st.is_repo = true;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            let rest = rest.trim();
            if rest == "(detached)" {
                st.detached = true;
                st.branch = None;
            } else {
                st.branch = Some(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            let mut it = rest.split_whitespace();
            let a = it.next().unwrap_or("+0").trim_start_matches('+');
            let b = it.next().unwrap_or("-0").trim_start_matches('-');
            st.ahead = a.parse().unwrap_or(0);
            st.behind = b.parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("? ") {
            st.untracked += 1;
            st.files.push(FileChange {
                path: rest.to_string(),
                index: '?',
                worktree: '?',
                status: "未跟踪".into(),
            });
        } else if let Some(rest) = line.strip_prefix("! ") {
            st.files.push(FileChange {
                path: rest.to_string(),
                index: '!',
                worktree: '!',
                status: "忽略".into(),
            });
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
            let kind = line.as_bytes()[0] as char;
            let bound = match kind {
                '1' => 9,
                '2' => 10,
                _ => 11,
            };
            let fields = line.splitn(bound, ' ').collect::<Vec<_>>();
            if fields.len() < 2 {
                continue;
            }
            let xy: Vec<char> = fields[1].chars().collect();
            if xy.len() < 2 {
                continue;
            }
            let (index, worktree) = (xy[0], xy[1]);
            let raw_path = if kind == '2' {
                fields
                    .last()
                    .copied()
                    .unwrap_or("")
                    .split('\t')
                    .next()
                    .unwrap_or("")
            } else {
                fields.last().copied().unwrap_or("")
            };
            if kind == 'u' {
                st.staged += 1;
                st.unstaged += 1;
            } else {
                if index != '.' && index != '?' && index != '!' {
                    st.staged += 1;
                }
                if worktree != '.' && worktree != '?' && worktree != '!' {
                    st.unstaged += 1;
                }
            }
            let status = if kind == 'u' {
                "冲突".to_string()
            } else {
                status_label(index, worktree)
            };
            st.files.push(FileChange {
                path: raw_path.to_string(),
                index,
                worktree,
                status,
            });
        }
    }
    st
}

pub fn repo_status(project_id: &str, dir: &Path) -> RepoStatus {
    if !dir.is_dir() {
        return RepoStatus::errored(project_id, format!("目录不存在：{}", dir.display()));
    }
    match run_git(dir, &["--no-optional-locks", "status", "--porcelain=v2", "--branch"]) {
        Ok(text) => parse_status(project_id, &text),
        Err(e) if e == "不是 git 仓库" => RepoStatus::not_repo(project_id),
        Err(e) => RepoStatus::errored(project_id, e),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub date: String,
    pub subject: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from_lane: u16,
    pub to_lane: u16,
    pub parent_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRow {
    pub commit: Commit,
    pub lane: u16,
    pub color: u8,
    pub passes: Vec<u16>,
    pub edges: Vec<GraphEdge>,
}

/// 输入 newest-first、含 parents 的提交列表，输出每行的泳道/贯穿线/跨道连线。
pub fn assign_lanes(commits: &[Commit]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut colors: Vec<u8> = Vec::new();
    let mut next_color: u8 = 0;
    let mut rows = Vec::with_capacity(commits.len());

    let alloc = |lanes: &mut Vec<Option<String>>, colors: &mut Vec<u8>, next: &mut u8| -> usize {
        match lanes.iter().position(|e| e.is_none()) {
            Some(i) => i,
            None => {
                lanes.push(None);
                colors.push(*next);
                *next = next.wrapping_add(1);
                lanes.len() - 1
            }
        }
    };

    for commit in commits {
        let matches: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, e)| e.as_deref() == Some(commit.hash.as_str()))
            .map(|(i, _)| i)
            .collect();
        let lane = match matches.first() {
            Some(&first) => {
                for &m in matches.iter().skip(1) {
                    lanes[m] = None;
                }
                first
            }
            None => alloc(&mut lanes, &mut colors, &mut next_color),
        };
        if lane >= colors.len() {
            colors.push(next_color);
            next_color = next_color.wrapping_add(1);
        }

        // first parent inherits this lane (vertical); extra parents branch out
        lanes[lane] = commit.parents.first().cloned();
        let mut edges = Vec::new();
        for parent in commit.parents.iter().skip(1) {
            let target = lanes
                .iter()
                .position(|e| e.as_deref() == Some(parent.as_str()))
                .unwrap_or_else(|| alloc(&mut lanes, &mut colors, &mut next_color));
            lanes[target] = Some(parent.clone());
            edges.push(GraphEdge { from_lane: lane as u16, to_lane: target as u16, parent_hash: parent.clone() });
        }

        let passes = lanes
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_some())
            .map(|(i, _)| i as u16)
            .collect();

        rows.push(GraphRow {
            commit: commit.clone(),
            lane: lane as u16,
            color: colors.get(lane).copied().unwrap_or(0),
            passes,
            edges,
        });
    }
    rows
}

pub fn parse_log(text: &str) -> Vec<Commit> {
    text.split('\u{1e}')
        .map(|rec| rec.trim_matches(|c| c == '\n' || c == '\r'))
        .filter(|rec| !rec.is_empty())
        .filter_map(parse_commit_record)
        .collect()
}

fn parse_commit_record(rec: &str) -> Option<Commit> {
    let f: Vec<&str> = rec.split('\u{1f}').collect();
    if f.len() < 8 {
        return None;
    }
    Some(Commit {
        hash: f[0].to_string(),
        short: f[1].to_string(),
        parents: f[2].split_whitespace().map(str::to_string).collect(),
        author: f[3].to_string(),
        email: f[4].to_string(),
        date: f[5].to_string(),
        subject: f[6].to_string(),
        refs: f[7].split(',').map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect(),
    })
}

const LOG_FORMAT: &str =
    "--pretty=format:%H%x1f%h%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%D%x1e";

fn log_result(result: Result<String, String>) -> Result<Vec<Commit>, String> {
    match result {
        Ok(text) => Ok(parse_log(&text)),
        Err(e) if e.contains("does not have any commits yet") => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

pub fn git_log(dir: &Path, limit: u32, skip: u32) -> Result<Vec<Commit>, String> {
    let limit = limit.min(200).to_string();
    let skip = skip.to_string();
    let text = run_git(
        dir,
        &["log", "--date-order", "--max-count", limit.as_str(), "--skip", skip.as_str(), LOG_FORMAT],
    );
    log_result(text)
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

    #[test]
    fn parse_status_branch_and_counts() {
        let text = "\
# branch.oid 1111111111111111111111111111111111111111
# branch.head main
# branch.upstream origin/main
# branch.ab +2 -1
1 .M N... 100644 100644 100644 aaaaaaa bbbbbbb src/app.ts
1 M. N... 100644 100644 100644 aaaaaaa bbbbbbb README.md
? notes.txt
! build/out.exe
";
        let st = parse_status("p1", text);
        assert!(st.is_repo);
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert!(!st.detached);
        assert_eq!((st.ahead, st.behind), (2, 1));
        assert_eq!(st.staged, 1);
        assert_eq!(st.unstaged, 1);
        assert_eq!(st.untracked, 1);
        assert_eq!(st.files.len(), 4);
        assert_eq!(st.files[0].path, "src/app.ts");
        assert_eq!(st.files[0].status, "修改");
        assert_eq!(st.files[3].status, "忽略");
    }

    #[test]
    fn parse_status_detached() {
        let text = "# branch.head (detached)\n# branch.oid abc\n";
        let st = parse_status("p1", text);
        assert!(st.detached);
        assert_eq!(st.branch, None);
        assert_eq!((st.ahead, st.behind), (0, 0));
    }

    #[test]
    fn parse_status_rename_uses_new_path() {
        let text = "2 R. N... 100644 100644 100644 aaaaaaa bbbbbbb R100 new name.ts\told name.ts\n";
        let st = parse_status("p1", text);
        assert_eq!(st.files[0].path, "new name.ts");
        assert_eq!(st.files[0].status, "重命名");
    }

    #[test]
    fn parse_status_keeps_spaces_in_path() {
        let text = "1 .M N... 100644 100644 100644 aaaaaaa bbbbbbb my dir/app.ts\n";
        let st = parse_status("p1", text);
        assert_eq!(st.files[0].path, "my dir/app.ts");
        assert_eq!(st.files[0].status, "修改");
    }

    #[test]
    fn parse_status_unmerged_is_conflict_and_counted_both() {
        let text = "u UU N... 100644 100644 100644 100644 a b c conflicted.txt\n";
        let st = parse_status("p1", text);
        assert_eq!(st.files[0].status, "冲突");
        assert_eq!(st.files[0].index, 'U');
        assert_eq!(st.files[0].worktree, 'U');
        assert_eq!(st.staged, 1);
        assert_eq!(st.unstaged, 1);
    }

    #[test]
    fn parse_status_unmerged_added_combo_is_conflict() {
        let text = "u AA N... 100644 100644 100644 100644 a b c conflicted.txt\n";
        let st = parse_status("p1", text);
        assert_eq!(st.files[0].status, "冲突");
        assert_eq!(st.staged, 1);
        assert_eq!(st.unstaged, 1);
    }

    #[test]
    fn repo_status_missing_dir_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        let st = repo_status("p1", &missing);
        assert!(!st.is_repo);
        assert!(st.error.unwrap().contains("目录不存在"));
    }

    #[test]
    fn parse_log_splits_records_and_parents() {
        let us = '\u{1f}';
        let rs = '\u{1e}';
        let text = format!(
            "H1{us}h1{us}P1 P2{us}Alice{us}a@x.com{us}2026-09-13T10:00:00+08:00{us}fix: 中文 主题{us}HEAD -> main, origin/main{rs}\n\
             H2{us}h2{us}{us}Bob{us}b@x.com{us}2026-09-12T10:00:00+08:00{us}init{us}{rs}\n"
        );
        let commits = parse_log(&text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "H1");
        assert_eq!(commits[0].parents, vec!["P1", "P2"]);
        assert_eq!(commits[0].subject, "fix: 中文 主题");
        assert_eq!(commits[0].refs, vec!["HEAD -> main", "origin/main"]);
        assert!(commits[1].parents.is_empty());
        assert!(commits[1].refs.is_empty());
    }

    #[test]
    fn git_log_unborn_head_is_empty_ok() {
        let err = "fatal: your current branch 'main' does not have any commits yet".to_string();
        assert!(log_result(Err(err)).unwrap().is_empty());
    }

    #[test]
    fn git_log_real_error_propagates() {
        assert_eq!(log_result(Err("不是 git 仓库".to_string())).unwrap_err(), "不是 git 仓库");
    }

    fn commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.into(),
            short: hash.into(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            author: "a".into(),
            email: "a@x".into(),
            date: "2026-09-13T00:00:00+08:00".into(),
            subject: "s".into(),
            refs: vec![],
        }
    }

    #[test]
    fn lanes_linear_history_single_lane() {
        let commits = [commit("C", &["B"]), commit("B", &["A"]), commit("A", &[])];
        let rows = assign_lanes(&commits);
        assert!(rows.iter().all(|r| r.lane == 0));
        assert_eq!(rows[0].passes, vec![0]); // below C the B lane continues
        assert_eq!(rows[2].passes, Vec::<u16>::new());
        assert!(rows.iter().all(|r| r.edges.is_empty()));
    }

    #[test]
    fn lanes_merge_has_cross_edge() {
        let commits = [commit("M", &["P1", "P2"]), commit("P1", &["A"]), commit("P2", &["A"]), commit("A", &[])];
        let rows = assign_lanes(&commits);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].edges.len(), 1);
        assert_ne!(rows[0].edges[0].from_lane, rows[0].edges[0].to_lane);
        assert_eq!(rows[0].edges[0].parent_hash, "P2");
        // both branch lanes are alive right after the merge
        assert_eq!(rows[0].passes, vec![0, 1]);
    }

    #[test]
    fn lanes_root_releases_lane() {
        let commits = [commit("A", &[])];
        let rows = assign_lanes(&commits);
        assert_eq!(rows[0].lane, 0);
        assert!(rows[0].passes.is_empty());
    }
}

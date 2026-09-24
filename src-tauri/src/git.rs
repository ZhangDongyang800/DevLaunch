use serde::Serialize;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// 只读调用的超时。
///
/// 5s 太紧：Windows 上杀软/索引器在场时，一个十万文件量级的仓库跑一次
/// `git status` 冷缓存就要好几秒，超时会被 `kill_tree` 强杀并让首页徽章
/// 变成灰色的 `—`——用户看到的是"随机出现的错误"，而不是"这个仓库很大"。
/// 读操作本来就不改仓库状态，放宽到 15s 的代价只是慢，收益是不再误报。
/// 写/网络路径各自在自己的模块里传 `GitRunOpts.timeout`，不受这里影响。
const READ_TIMEOUT: Duration = Duration::from_secs(15);
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

/// 用户在「设置 → Git 可执行文件」指定的路径（进程级）。空/None = 自动检测。
static CONFIGURED_GIT: Mutex<Option<String>> = Mutex::new(None);

pub fn set_configured_git(path: Option<&str>) {
    if let Ok(mut g) = CONFIGURED_GIT.lock() {
        *g = path.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    }
}

pub fn configured_git() -> Option<String> {
    CONFIGURED_GIT.lock().ok().and_then(|g| g.clone())
}

/// 解析优先级：配置路径（显式，若无效则判定缺失，不回退）→ `DEVLAUNCH_GIT_PATH`
/// （空串 = 强制缺失）→ PATH → `%ProgramFiles%\Git\cmd|bin\git.exe`。
pub fn resolve_git_effective(
    configured: Option<&str>,
    env_override: Option<&OsStr>,
    path_env: Option<&OsStr>,
    program_files: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(c) = configured {
        let c = c.trim();
        if !c.is_empty() {
            let p = PathBuf::from(c);
            return p.is_file().then_some(p);
        }
    }
    resolve_git_with(env_override, path_env, program_files)
}

pub fn resolve_git_path() -> Option<PathBuf> {
    let configured = configured_git();
    resolve_git_effective(
        configured.as_deref(),
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
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    // 优先取最后一条 error:/fatal:——hook 的输出与 hint: 续行常排在真正的错误后面。
    let picked = lines
        .iter()
        .rev()
        .find(|l| l.starts_with("error:") || l.starts_with("fatal:"))
        .or_else(|| lines.last())
        .copied()
        .unwrap_or("git 执行失败");
    picked.to_string()
}

#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

/// 管道读取缓冲：读线程把数据搬进共享缓冲，主线程随时可取已读到的部分。
/// 这样即便孙进程继承了管道句柄、读线程等不到 EOF，调用方也不会被永久卡住。
struct PipeBuf {
    data: Arc<Mutex<VecDeque<u8>>>,
    done: Arc<AtomicBool>,
}

impl PipeBuf {
    /// `head`：保留前 `limit` 字节并在达到上限时置位 `capped`（stdout，超出即无意义）。
    /// `tail`：保留最后 `limit` 字节（stderr，git 的 error:/fatal: 总在末尾）。
    fn spawn<R: Read + Send + 'static>(
        mut pipe: R,
        limit: usize,
        head: Option<Arc<AtomicBool>>,
    ) -> Self {
        let data = Arc::new(Mutex::new(VecDeque::new()));
        let done = Arc::new(AtomicBool::new(false));
        let (buf, flag) = (Arc::clone(&data), Arc::clone(&done));
        std::thread::spawn(move || {
            let mut chunk = [0u8; 16 * 1024];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let reached_limit = {
                            let mut v = buf.lock().unwrap_or_else(|e| e.into_inner());
                            if head.is_some() {
                                let room = limit.saturating_sub(v.len());
                                v.extend(&chunk[..n.min(room)]);
                                v.len() >= limit
                            } else {
                                v.extend(&chunk[..n]);
                                while v.len() > limit {
                                    v.pop_front();
                                }
                                false
                            }
                        };
                        if reached_limit {
                            if let Some(c) = &head {
                                c.store(true, Ordering::SeqCst);
                            }
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            flag.store(true, Ordering::SeqCst);
        });
        Self { data, done }
    }

    /// 最多再等 grace；到期则返回已读到的部分，读线程留在后台自然收尾。
    fn collect(&self, grace: Duration) -> Vec<u8> {
        let deadline = Instant::now() + grace;
        while !self.done.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.data.lock().map(|v| v.iter().copied().collect()).unwrap_or_default()
    }
}

/// 有界等待一个子进程退出，超时后强杀它自己。
fn wait_bounded(child: &mut std::process::Child, limit: Duration) {
    let start = Instant::now();
    while child.try_wait().ok().flatten().is_none() {
        if start.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 强杀 git **及其子进程树**：hook、Git Credential Manager、ssh 都是 git 的子进程。
/// 只杀 git.exe 会留下继承了管道句柄的孙进程，读端永远不 EOF，`git_op` 锁会被永久占住。
#[cfg(windows)]
fn kill_tree(child: &mut std::process::Child) {
    let pid = child.id();
    if pid > 0 {
        if let Ok(mut tk) = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        {
            wait_bounded(&mut tk, Duration::from_secs(5));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(windows))]
fn kill_tree(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// git 子进程运行档。`allow_prompt=false` 时禁用终端提示（读取与本地写），
/// `true` 时留给 Git Credential Manager（网络操作）。
pub struct GitRunOpts {
    pub allow_prompt: bool,
    pub timeout: Duration,
    pub stdin_data: Option<Vec<u8>>,
}

impl Default for GitRunOpts {
    fn default() -> Self {
        Self { allow_prompt: false, timeout: READ_TIMEOUT, stdin_data: None }
    }
}

/// 一次 git 运行的原始产物；管道已由读线程接管，取用一律有界。
struct GitRaw {
    out: Vec<u8>,
    err: Vec<u8>,
    capped: bool,
    exit_ok: bool,
    timed_out: bool,
    started_at: std::time::SystemTime,
}

/// 只读/内部用：不做锁文件清理（清理路径自身也要跑 git，避免递归）。
pub(crate) fn run_git_with(program: &Path, dir: &Path, args: &[&str]) -> Result<String, String> {
    let raw = spawn_git(program, dir, args, GitRunOpts::default())?;
    raw.into_result(None)
}

/// 只读调用的产物：文本 + 是否因为撞上输出上限而被截断。
///
/// 截断必须能被调用方看见：`capped_result` 在超限时返回的是 `Ok`（内容不完整但
/// 语法上仍能解析），`parse_status` / `parse_log` 会把残缺的尾部当完整数据用，
/// 结果是脏文件数、ahead/behind、提交列表**静默偏少**。有了这个标记，调用方
/// 才能如实告诉用户"结果不完整"。
pub(crate) struct GitText {
    pub text: String,
    pub capped: bool,
}

pub(crate) fn run_git_capped(program: &Path, dir: &Path, args: &[&str]) -> Result<GitText, String> {
    let raw = spawn_git(program, dir, args, GitRunOpts::default())?;
    let capped = raw.capped;
    raw.into_result(None).map(|text| GitText { text, capped })
}

pub(crate) fn run_git_with_opts(
    program: &Path,
    dir: &Path,
    args: &[&str],
    opts: GitRunOpts,
) -> Result<String, String> {
    let raw = spawn_git(program, dir, args, opts)?;
    // 写操作被强杀后可能留下 index.lock，不清会导致后续所有写一直失败。
    let note = (raw.timed_out || raw.capped)
        .then(|| cleanup_killed_index_lock(program, dir, raw.started_at))
        .flatten();
    raw.into_write_result(note)
}

impl GitRaw {
    fn into_write_result(self, note: Option<String>) -> Result<String, String> {
        if self.capped {
            let mut msg = format!(
                "git 输出超过 {} MB 上限，命令已强制终止；命令结果可能已部分生效",
                MAX_OUTPUT_BYTES / 1024 / 1024
            );
            if let Some(note) = note {
                msg.push('；');
                msg.push_str(&note);
            }
            return Err(msg);
        }
        if self.timed_out {
            let mut msg = "git 执行超时（已强制结束 Git 进程）；命令结果可能已部分生效".to_string();
            if let Some(note) = note {
                msg.push('；');
                msg.push_str(&note);
            }
            return Err(msg);
        }
        self.into_result(note)
    }

    fn into_result(self, note: Option<String>) -> Result<String, String> {
        let stderr = String::from_utf8_lossy(&self.err).into_owned();
        // git 把「no changes added to commit」这类结论写在 stdout，此时 stderr 里
        // 往往只有钩子噪音；拿 stdout 的最后一行兜底，否则用户看到的是无意义的一行。
        let stderr = if !self.exit_ok && !has_diagnostic_line(&stderr) {
            let stdout = String::from_utf8_lossy(&self.out).into_owned();
            match stdout.lines().rev().map(str::trim).find(|l| !l.is_empty()) {
                Some(last) => last.to_string(),
                None => stderr,
            }
        } else {
            stderr
        };
        capped_result(self.out, self.capped, self.exit_ok, self.timed_out, &stderr, note)
    }
}

fn has_diagnostic_line(stderr: &str) -> bool {
    stderr.lines().any(|l| {
        let l = l.trim();
        l.starts_with("error:") || l.starts_with("fatal:")
    })
}

fn spawn_git(
    program: &Path,
    dir: &Path,
    args: &[&str],
    opts: GitRunOpts,
) -> Result<GitRaw, String> {
    let mut cmd = Command::new(program);
    cmd.arg("-C")
        .arg(dir)
        .arg("--no-pager")
        .arg("-c")
        .arg("color.ui=false")
        .arg("-c")
        .arg("core.quotepath=false")
        .args(args)
        .stdin(if opts.stdin_data.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_OPTIONAL_LOCKS", "0");
    if !opts.allow_prompt {
        cmd.env("GIT_TERMINAL_PROMPT", "0");
    }
    hide_console(&mut cmd);

    let started_at = std::time::SystemTime::now();
    let mut child = cmd.spawn().map_err(|e| format!("启动 git 失败：{e}"))?;

    // 读线程必须先于 stdin 写入启动：git 或它的 hook 可能在读 stdin 之前就把
    // stdout/stderr 灌满管道缓冲区，此时同步写 stdin 会双向死锁。
    let out_pipe = child.stdout.take().expect("piped stdout");
    let err_pipe = child.stderr.take().expect("piped stderr");
    let capped = Arc::new(AtomicBool::new(false));
    // 多读 1 字节用于区分「正好到上限」与「被截断」。
    let out_buf = PipeBuf::spawn(
        out_pipe,
        (MAX_OUTPUT_BYTES + 1) as usize,
        Some(Arc::clone(&capped)),
    );
    let err_buf = PipeBuf::spawn(err_pipe, 64 * 1024, None);
    if let Some(data) = opts.stdin_data {
        if let Some(mut si) = child.stdin.take() {
            std::thread::spawn(move || {
                use std::io::Write;
                let _ = si.write_all(&data);
                // si 在此 drop，关闭 stdin，git 才会继续往下走
            });
        }
    }

    let start = Instant::now();
    let mut exit_ok = false;
    let mut timed_out = false;
    loop {
        if capped.load(Ordering::SeqCst) {
            // 输出超限：提前结束，不再等超时。
            kill_tree(&mut child);
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_ok = status.success();
                break;
            }
            Ok(None) => {
                if start.elapsed() >= opts.timeout {
                    kill_tree(&mut child);
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                kill_tree(&mut child);
                timed_out = true;
                break;
            }
        }
    }

    // 进程已退出或已被强杀；再给读线程一个有界窗口收尾管道里剩下的数据。
    let grace = if timed_out { Duration::from_millis(500) } else { Duration::from_secs(2) };
    Ok(GitRaw {
        out: out_buf.collect(grace),
        err: err_buf.collect(grace),
        capped: capped.load(Ordering::SeqCst),
        exit_ok,
        timed_out,
        started_at,
    })
}

pub fn run_git_opts(dir: &Path, args: &[&str], opts: GitRunOpts) -> Result<String, String> {
    let program = resolve_git_path().ok_or_else(|| {
        "未找到 git.exe；请在「设置 → Git 可执行文件」指定路径，或安装 Git for Windows".to_string()
    })?;
    run_git_with_opts(&program, dir, args, opts)
}

fn capped_result(
    out: Vec<u8>,
    capped: bool,
    exit_ok: bool,
    timed_out: bool,
    stderr: &str,
    note: Option<String>,
) -> Result<String, String> {
    if capped {
        let end = (MAX_OUTPUT_BYTES as usize).min(out.len());
        return Ok(String::from_utf8_lossy(&out[..end]).into_owned());
    }
    if timed_out {
        let mut msg = "git 执行超时（已强制结束 Git 进程）".to_string();
        if let Some(note) = note {
            msg.push('；');
            msg.push_str(&note);
        }
        return Err(msg);
    }
    if exit_ok {
        Ok(String::from_utf8_lossy(&out).into_owned())
    } else {
        Err(friendly_git_error(stderr))
    }
}

/// Windows 下以独占方式试探打开文件：别的进程还持有它时返回 false。
#[cfg(windows)]
fn file_is_free(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .is_ok()
}

#[cfg(not(windows))]
fn file_is_free(_path: &Path) -> bool {
    true
}

/// 被我们强杀的 git 不会走自己的清理路径，可能留下 `.git/index.lock`，
/// 之后所有写操作都会一直报「锁已存在」。只在超时强杀后调用，且两道防线：
/// 锁必须晚于本次运行开始（早于它的属于别的进程），并且当前无人持有句柄。
fn cleanup_killed_index_lock(program: &Path, dir: &Path, started_at: std::time::SystemTime) -> Option<String> {
    let Ok(git_dir) = run_git_with(program, dir, &["rev-parse", "--absolute-git-dir"]) else {
        return None;
    };
    let lock = PathBuf::from(git_dir.trim()).join("index.lock");
    let Ok(meta) = std::fs::metadata(&lock) else { return None };
    let Ok(mtime) = meta.modified() else { return None };
    if mtime < started_at {
        return Some("索引锁由其他 Git 操作持有，未清理".into());
    }
    if !file_is_free(&lock) {
        return Some("索引锁仍被占用，未清理".into());
    }
    match std::fs::remove_file(&lock) {
        Ok(_) => Some("已清理中断残留的 index.lock".into()),
        Err(_) => Some(format!("索引锁未释放，请手动删除：{}", lock.display())),
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
    pub conflicts: u32,
    pub operation: Option<String>,
    /// `git status` 的输出撞上 4MB 上限被截断 → 上面的计数与 `files` 都不完整。
    /// 必须让前端能显示出来，否则"少列了几个文件"会被当成"就这些"。
    pub truncated: bool,
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
            conflicts: 0,
            operation: None,
            truncated: false,
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
                st.conflicts += 1;
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

/// 依据 `.git` 目录下的标记文件判断进行中的操作（只读）。
pub fn operation_from_git_dir(git_dir: &Path) -> Option<String> {
    if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
        return Some("rebase".into());
    }
    if git_dir.join("MERGE_HEAD").is_file() {
        return Some("merge".into());
    }
    if git_dir.join("CHERRY_PICK_HEAD").is_file() {
        return Some("cherry-pick".into());
    }
    None
}

/// worktree 与 worktree 内的子目录都能解析到真正的 git dir（处理 `.git` 是文件的情况）。
pub fn in_progress(dir: &Path) -> Option<String> {
    let git_dir = run_git(dir, &["rev-parse", "--git-dir"]).ok()?;
    let p = PathBuf::from(git_dir.trim());
    let abs = if p.is_absolute() { p } else { dir.join(p) };
    operation_from_git_dir(&abs)
}

pub fn repo_status(project_id: &str, dir: &Path) -> RepoStatus {
    let program = resolve_git_path();
    repo_status_with(project_id, dir, program.as_deref())
}

pub fn repo_status_with(project_id: &str, dir: &Path, program: Option<&Path>) -> RepoStatus {
    if !dir.is_dir() {
        return RepoStatus::errored(project_id, format!("目录不存在：{}", dir.display()));
    }
    // 缺 git.exe 必须是「错误」，不能与「非仓库」混为一谈。
    let Some(program) = program else {
        return RepoStatus::errored(
            project_id,
            "未找到 git.exe；请在「设置 → Git 可执行文件」指定路径，或安装 Git for Windows".into(),
        );
    };
    // Locale-independent work-tree check; erroring here means "not a repo".
    if run_git_with(program, dir, &["rev-parse", "--is-inside-work-tree"]).is_err() {
        return RepoStatus::not_repo(project_id);
    }
    match run_git_capped(program, dir, &["--no-optional-locks", "status", "--porcelain=v2", "--branch"]) {
        Ok(got) => {
            let mut st = parse_status(project_id, &got.text);
            st.truncated = got.capped;
            st.operation = in_progress(dir);
            st
        }
        Err(e) => RepoStatus::errored(project_id, e),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub current: bool,
    pub remote: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// 解析 `for-each-ref --format=%(refname:short)%x1f%(upstream:short)%x1f%(upstream:track)`。
pub fn parse_branches(text: &str, current: Option<&str>) -> Vec<BranchInfo> {
    let mut out: Vec<BranchInfo> = text
        .split('\n')
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut f = line.split('\u{1f}');
            let name = f.next().unwrap_or("").to_string();
            let upstream = f.next().unwrap_or("").trim();
            let track = f.next().unwrap_or("").trim();
            let mut ahead = 0;
            let mut behind = 0;
            if let Some(inner) = track.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                for part in inner.split(',') {
                    let part = part.trim();
                    if let Some(v) = part.strip_prefix("ahead ") {
                        ahead = v.trim().parse().unwrap_or(0);
                    } else if let Some(v) = part.strip_prefix("behind ") {
                        behind = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            BranchInfo {
                current: current == Some(name.as_str()),
                name,
                remote: false,
                upstream: if upstream.is_empty() { None } else { Some(upstream.to_string()) },
                ahead,
                behind,
            }
        })
        .collect();
    // 本地分支排在远程前面。
    out.sort_by(|a, b| (a.remote, &a.name).cmp(&(b.remote, &b.name)));
    out
}

/// 解析 `for-each-ref refs/remotes`（`name` 形如 `origin/main`；过滤 `*/HEAD`）。
pub fn parse_remote_branches(text: &str) -> Vec<BranchInfo> {
    text.split('\n')
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut it = line.split('\u{1f}');
            let name = it.next().unwrap_or("").trim();
            // refs/remotes/origin 本身是「远程 HEAD」符号引用，%(refname:short) 会把它
            // 显示成光秃秃的 "origin"，选它只会报「分支名为空」，必须滤掉。
            let symbolic = it.next().map(|s| !s.trim().is_empty()).unwrap_or(false);
            (name, symbolic)
        })
        .filter(|(name, symbolic)| !symbolic && !name.is_empty() && !name.ends_with("/HEAD"))
        .map(|(name, _)| BranchInfo {
            name: name.to_string(),
            current: false,
            remote: true,
            upstream: None,
            ahead: 0,
            behind: 0,
        })
        .collect()
}

pub fn branches(dir: &Path) -> Result<Vec<BranchInfo>, String> {
    let locals = run_git(
        dir,
        &[
            "for-each-ref",
            "refs/heads",
            "--format=%(refname:short)\u{1f}%(upstream:short)\u{1f}%(upstream:track)",
        ],
    )?;
    let current = run_git(dir, &["symbolic-ref", "--short", "HEAD"]).ok();
    let mut out = parse_branches(&locals, current.as_deref().map(str::trim));
    if let Ok(remotes) = run_git(
        dir,
        &["for-each-ref", "refs/remotes", "--format=%(refname:short)\u{1f}%(symref)"],
    ) {
        out.extend(parse_remote_branches(&remotes));
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: String,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub staged: bool,
    pub untracked: bool,
    pub binary: bool,
    pub truncated: bool,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<Hunk>,
}

fn parse_hunk_starts(rest: &str) -> (u32, u32) {
    let mut old = 0;
    let mut new = 0;
    for part in rest.split(' ') {
        if let Some(v) = part.strip_prefix('-') {
            old = v.split(',').next().unwrap_or("0").parse().unwrap_or(0);
        } else if let Some(v) = part.strip_prefix('+') {
            new = v.split(',').next().unwrap_or("0").parse().unwrap_or(0);
        }
    }
    (old, new)
}

fn flush_hunk(cur: &mut Option<FileDiff>, hunk: &mut Option<Hunk>) {
    if let (Some(f), Some(h)) = (cur.as_mut(), hunk.take()) {
        f.hunks.push(h);
    }
}

fn flush_file(files: &mut Vec<FileDiff>, cur: &mut Option<FileDiff>, hunk: &mut Option<Hunk>) {
    flush_hunk(cur, hunk);
    if let Some(f) = cur.take() {
        files.push(f);
    }
}

/// 解析 `git diff`/`git show` 的统一 diff 文本为结构化文件列表。
pub fn parse_unified_diff(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut hunk: Option<Hunk> = None;
    let mut old_no = 0u32;
    let mut new_no = 0u32;

    for line in text.lines() {
        if line.starts_with("diff --git ") {
            flush_file(&mut files, &mut cur, &mut hunk);
            cur = Some(FileDiff {
                path: String::new(),
                staged: false,
                untracked: false,
                binary: false,
                truncated: false,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
            continue;
        }
        if cur.is_none() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            flush_hunk(&mut cur, &mut hunk);
            if let Some(f) = cur.as_mut() {
                if let Some(p) = rest.strip_prefix("b/") {
                    f.path = p.to_string();
                } else if rest != "/dev/null" {
                    f.path = rest.to_string();
                }
            }
        } else if line.starts_with("Binary files ") && line.ends_with(" differ") {
            if let Some(f) = cur.as_mut() {
                f.binary = true;
            }
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            flush_hunk(&mut cur, &mut hunk);
            let (os, ns) = parse_hunk_starts(rest);
            old_no = os;
            new_no = ns;
            hunk = Some(Hunk { header: format!("@@ {rest}"), old_start: os, new_start: ns, lines: Vec::new() });
        } else if let Some(h) = hunk.as_mut() {
            if line.starts_with('\\') {
                continue; // \ No newline at end of file
            }
            let (kind, content) = match line.as_bytes().first() {
                Some(b'+') => ("add", &line[1..]),
                Some(b'-') => ("del", &line[1..]),
                Some(b' ') => ("context", &line[1..]),
                _ => continue,
            };
            let (old_n, new_n) = match kind {
                "add" => (None, Some(new_no)),
                "del" => (Some(old_no), None),
                _ => (Some(old_no), Some(new_no)),
            };
            if kind != "add" {
                old_no += 1;
            }
            if kind != "del" {
                new_no += 1;
            }
            if let Some(f) = cur.as_mut() {
                if kind == "add" {
                    f.additions += 1;
                } else if kind == "del" {
                    f.deletions += 1;
                }
            }
            h.lines.push(DiffLine { kind: kind.into(), old_no: old_n, new_no: new_n, text: content.to_string() });
        }
    }
    flush_file(&mut files, &mut cur, &mut hunk);
    files
}

/// git 自身的判据（`Buffer::is_binary`）：前 8000 字节里出现 NUL 即二进制。
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|&b| b == 0)
}

fn read_file_limited(path: &Path, limit: usize) -> Result<(Vec<u8>, bool), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("读取文件失败：{e}"))?;
    let mut reader = file.take(limit.saturating_add(1) as u64);
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    reader.read_to_end(&mut bytes).map_err(|e| format!("读取文件失败：{e}"))?;
    let truncated = bytes.len() > limit;
    bytes.truncate(limit);
    Ok((bytes, truncated))
}

pub(crate) fn repo_file_for_read(dir: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
            )
        })
    {
        return Err(format!("非法文件路径：{path}"));
    }
    let root = std::fs::canonicalize(dir).map_err(|e| format!("解析仓库目录失败：{e}"))?;
    let target = std::fs::canonicalize(dir.join(relative)).map_err(|e| format!("解析仓库文件失败：{e}"))?;
    if !target.starts_with(&root) {
        return Err(format!("文件指向仓库外，已拒绝读取：{path}"));
    }
    if !std::fs::metadata(&target).map_err(|e| format!("读取文件属性失败：{e}"))?.is_file() {
        return Err(format!("不是文件：{path}"));
    }
    Ok(target)
}

/// 未跟踪文件的“diff”= 整文件视为新增（内容预览，≤256KB）。二进制内容不做 lossy 文本渲染。
pub fn untracked_file_diff(dir: &Path, path: &str) -> Result<FileDiff, String> {
    let abs = repo_file_for_read(dir, path)?;
    let (bytes, oversized) = read_file_limited(&abs, MAX_PATCH_BYTES)?;
    if looks_binary(&bytes) {
        return Ok(FileDiff {
            path: path.to_string(),
            staged: false,
            untracked: true,
            binary: true,
            truncated: oversized,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
        });
    }
    let (text, truncated_text) = truncate_patch(String::from_utf8_lossy(&bytes).into_owned());
    let truncated = oversized || truncated_text;
    let total = text.split('\n').count();
    let mut lines = Vec::new();
    let mut n = 0u32;
    for (i, raw) in text.split('\n').enumerate() {
        if i + 1 == total && raw.is_empty() {
            continue; // 末尾换行产生的空段
        }
        n += 1;
        lines.push(DiffLine {
            kind: "add".into(),
            old_no: None,
            new_no: Some(n),
            text: raw.trim_end_matches('\r').to_string(),
        });
    }
    let hunks = if lines.is_empty() {
        Vec::new()
    } else {
        vec![Hunk { header: "@@ 新文件 @@".into(), old_start: 0, new_start: 1, lines }]
    };
    Ok(FileDiff {
        path: path.to_string(),
        staged: false,
        untracked: true,
        binary: false,
        truncated,
        additions: n,
        deletions: 0,
        hunks,
    })
}

pub fn file_diff(
    dir: &Path,
    path: &str,
    staged: bool,
    ignore_whitespace: bool,
    full_context: bool,
) -> Result<FileDiff, String> {
    // 已跟踪判定：ls-files --error-unmatch 对未跟踪文件返回非零。
    let tracked = run_git(dir, &["ls-files", "--error-unmatch", "--", path]).is_ok();
    if !tracked {
        return untracked_file_diff(dir, path);
    }
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    if ignore_whitespace {
        args.push("-w");
    }
    if full_context {
        args.push("--unified=100000");
    }
    args.extend(["--", path]);
    let raw = run_git(dir, &args)?;
    let (raw, truncated) = truncate_patch(raw);
    let mut parsed = parse_unified_diff(&raw);
    let mut f = parsed.pop().unwrap_or(FileDiff {
        path: path.to_string(),
        staged,
        untracked: false,
        binary: false,
        truncated: false,
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
    });
    f.path = path.to_string();
    f.staged = staged;
    f.truncated = truncated;
    Ok(f)
}

// ———— 二进制文件的内容预览（图片差异）————
//
// 文本 diff 的前提是「行」有意义；二进制没有行可言，所以 git 只报
// "Binary files … differ"。但内容并没有丢：git 是内容寻址存储，任意版本的
// 原始字节都可及（`HEAD:<path>`、索引 `:<path>`、`<hash>:<path>`），工作区那一
// 侧就在磁盘上。图片又是自描述的字节串，WebView 自带解码器——所以「预览二进制
// 差异」= 把两侧原样字节交给浏览器渲染，像素级差异由前端 canvas 算。

/// 单侧预览上限。base64 后 ×1.33 且两侧都进 IPC JSON，再大就是卡顿而不是预览。
pub const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

/// 值得读出来嗅探魔数的扩展名。它只决定「要不要花这次读取」，**不是**判据——
/// 判据是魔数（扩展名会撒谎）。代价：改名的图片（扩展名不对）不预览。
const IMAGE_EXTS: &[&str] =
    &["png", "apng", "jpg", "jpeg", "jfif", "gif", "bmp", "webp", "avif", "ico", "cur"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobSide {
    pub size: u64,
    /// 魔数确认的图片 MIME；None = 不是受支持的图片格式
    pub mime: Option<String>,
    /// 浏览器可直接解码的 data URL；None = 非图片、超过上限或读取失败
    pub data_url: Option<String>,
    /// 字节数或像素数超限——前端据此显示「超出预览上限」。
    /// 两者合一是有意的：对用户来说"看不了"是同一件事。
    pub too_big: bool,
    /// 从文件头读出的像素尺寸（不解码）；认不出格式时为 None
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// 单独标出"解码后太大"，与"文件本身太大"区分，便于给出准确的原因
    pub over_pixels: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryPreview {
    pub path: String,
    /// 至少一侧被魔数确认为图片 → 前端走图片视图；否则只有字节数
    pub image: bool,
    pub old: Option<BlobSide>,
    pub new: Option<BlobSide>,
}

pub fn has_image_extension(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) if i > 0 => IMAGE_EXTS.contains(&name[i + 1..].to_ascii_lowercase().as_str()),
        _ => false,
    }
}

/// 魔数 → 浏览器可解码的 MIME。只认 Chromium 解得动的格式（SVG 是文本，走文本 diff）。
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some("image/png"); // APNG 同魔数，浏览器一并解
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.len() >= 11 && &bytes[4..8] == b"ftyp" && matches!(&bytes[8..11], b"avi" | b"avs") {
        return Some("image/avif");
    }
    // ICO/CUR：保留位=0 + 类型 1/2 + 条目数非零
    if bytes.len() >= 6 && bytes.starts_with(&[0, 0, 1, 0]) && bytes[4..6] != [0, 0] {
        return Some("image/x-icon");
    }
    // BMP 的魔数只有两字节，太容易撞上任意二进制：再要求头部声明长度与实际字节数一致。
    if bytes.len() >= 14 && bytes.starts_with(b"BM") {
        let declared = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]) as u64;
        if declared == bytes.len() as u64 {
            return Some("image/bmp");
        }
    }
    None
}

/// 二进制安全的 git 读取。`run_git` 返回 lossy UTF-8，会改掉任何非文本字节，
/// 预览要的是原样字节，所以必须另走一条不转码的路。
/// `Ok(None)` = 输出触到管道上限（内容不完整，不可信）。
fn run_git_bytes(dir: &Path, args: &[&str]) -> Result<Option<Vec<u8>>, String> {
    let program = resolve_git_path().ok_or_else(|| {
        "未找到 git.exe；请在「设置 → Git 可执行文件」指定路径，或安装 Git for Windows".to_string()
    })?;
    let raw = spawn_git(&program, dir, args, GitRunOpts::default())?;
    if raw.timed_out {
        return Err("git 执行超时（已强制结束 Git 进程）".into());
    }
    if !raw.exit_ok {
        return Err(friendly_git_error(&String::from_utf8_lossy(&raw.err)));
    }
    Ok(if raw.capped { None } else { Some(raw.out) })
}

/// revspec → (对象 id, 字节数)。该版本没有这个文件时解析失败，是「这一侧不存在」而非错误。
fn object_of(dir: &Path, revspec: &str) -> Option<(String, u64)> {
    let sha = run_git(dir, &["rev-parse", "--verify", "--quiet", revspec]).ok()?;
    let sha = sha.trim().to_string();
    if sha.len() < 4 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let size = run_git(dir, &["cat-file", "-s", sha.as_str()]).ok()?;
    Some((sha, size.trim().parse().ok()?))
}

/// 单侧预览的**像素**上限。
///
/// 4MB 的字节上限约束不了内存：一张 9000×9000 的纯色 PNG 压缩后可能只有几百 KB，
/// 但 WebView 解码成 RGBA 位图是 324MB，前端再为逐像素比较复制两份就是 1GB 量级
/// ——足够让 WebView2 直接崩，而用户正在编辑的提交信息一起丢。
/// 所以字节之外必须再加一道"解码后有多大"的预检：只读文件头几十个字节就能算出来，
/// 超限时**不回传 data URL**，只报尺寸与字节数。
pub const MAX_PREVIEW_PIXELS: u64 = 40_000_000;

/// 从图片头部读尺寸（不解码）。认不出来就返回 None——此时只靠字节上限兜底。
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // PNG：IHDR 紧跟签名(8) + 长度(4) + 类型(4)
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
            let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
            let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
            return nonzero_dim(w, h);
        }
        return None;
    }
    // GIF：逻辑屏幕描述符
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        if bytes.len() >= 10 {
            let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
            let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
            return nonzero_dim(w, h);
        }
        return None;
    }
    // BMP：BITMAPINFOHEADER 的宽高（有符号；负高表示自上而下）
    if bytes.starts_with(b"BM") && bytes.len() >= 26 {
        let w = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
        let h = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]);
        return nonzero_dim(w.unsigned_abs(), h.unsigned_abs());
    }
    // JPEG：扫段找 SOFn
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return jpeg_dimensions(bytes);
    }
    // ICO/CUR：目录项里的宽高是单字节，0 表示 256
    if bytes.len() >= 8 && bytes.starts_with(&[0, 0, 1, 0]) {
        let w = if bytes[6] == 0 { 256 } else { u32::from(bytes[6]) };
        let h = if bytes[7] == 0 { 256 } else { u32::from(bytes[7]) };
        return nonzero_dim(w, h);
    }
    // WebP：VP8X / VP8（有损）/ VP8L（无损）
    if bytes.len() >= 30 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return webp_dimensions(bytes);
    }
    None
}

fn nonzero_dim(w: u32, h: u32) -> Option<(u32, u32)> {
    (w > 0 && h > 0).then_some((w, h))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2usize;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        // 填充字节与不带长度字段的标记
        if marker == 0xff || marker == 0x01 || (0xd0..=0xd9).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        // SOF0..SOF15，排除 DHT(0xC4) / JPG(0xC8) / DAC(0xCC)——它们不是 SOF
        let is_sof =
            (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc;
        if is_sof && i + 9 < bytes.len() {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return nonzero_dim(w, h);
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match &bytes[12..16] {
        b"VP8X" => {
            let w = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]) + 1;
            let h = u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) + 1;
            nonzero_dim(w, h)
        }
        b"VP8 " => {
            // 有损：帧同步码 9d 01 2a 之后是 14 位宽高
            let start = (16..bytes.len().saturating_sub(7))
                .find(|&i| bytes[i..i + 3] == [0x9d, 0x01, 0x2a])?;
            let w = u32::from(u16::from_le_bytes([bytes[start + 3], bytes[start + 4]])) & 0x3fff;
            let h = u32::from(u16::from_le_bytes([bytes[start + 5], bytes[start + 6]])) & 0x3fff;
            nonzero_dim(w, h)
        }
        b"VP8L" => {
            // 无损：签名 2f 之后 28 位里 14 位宽 + 14 位高
            if bytes[20] != 0x2f {
                return None;
            }
            let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
            nonzero_dim((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        _ => None,
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    use base64::{engine::general_purpose, Engine as _};
    format!("data:{mime};base64,{}", general_purpose::STANDARD.encode(bytes))
}

fn make_side(size: u64, limit: u64, bytes: Option<Vec<u8>>) -> BlobSide {
    let mime = bytes.as_deref().and_then(sniff_image).map(str::to_string);
    let dims = bytes.as_deref().and_then(image_dimensions);
    // 字节数过关不代表解码后过关：高压缩比的大图会把 WebView 拖垮。
    let over_pixels = dims
        .map(|(w, h)| u64::from(w) * u64::from(h) > MAX_PREVIEW_PIXELS)
        .unwrap_or(false);
    let url = match (&mime, &bytes) {
        (Some(m), Some(b)) if !over_pixels => Some(data_url(m, b)),
        _ => None,
    };
    BlobSide {
        size,
        mime,
        data_url: url,
        // 前端只看这一个字段决定要不要显示「超出预览上限」，所以两种超限都要算进来。
        too_big: size > limit || over_pixels,
        width: dims.map(|(w, _)| w),
        height: dims.map(|(_, h)| h),
        over_pixels,
    }
}

fn read_object_side(dir: &Path, specs: &[String], limit: u64, fetch: bool) -> Option<BlobSide> {
    let (sha, size) = specs.iter().find_map(|s| object_of(dir, s))?;
    let bytes = if fetch && size <= limit {
        run_git_bytes(dir, &["cat-file", "blob", sha.as_str()]).ok().flatten()
    } else {
        None
    };
    Some(make_side(size, limit, bytes))
}

fn read_worktree_side(dir: &Path, path: &str, limit: u64, fetch: bool) -> Option<BlobSide> {
    let abs = repo_file_for_read(dir, path).ok()?;
    let mut size = std::fs::metadata(&abs).ok()?.len();
    let bytes = if fetch && size <= limit {
        let byte_limit = usize::try_from(limit).ok()?;
        match read_file_limited(&abs, byte_limit) {
            Ok((bytes, false)) => Some(bytes),
            Ok((_, true)) => {
                size = size.max(limit.saturating_add(1));
                None
            }
            Err(_) => None,
        }
    } else {
        None
    };
    Some(make_side(size, limit, bytes))
}

/// `hash`=Some → 该提交 vs 第一父（History 页）；否则沿用 Changes 页语义：
/// 暂存侧比 HEAD↔索引，未暂存/未跟踪侧比索引（回落 HEAD）↔工作区磁盘文件。
pub fn binary_preview(
    dir: &Path,
    path: &str,
    staged: bool,
    hash: Option<&str>,
    limit: u64,
) -> Result<BinaryPreview, String> {
    resolve_git_path().ok_or_else(|| {
        "未找到 git.exe；请在「设置 → Git 可执行文件」指定路径，或安装 Git for Windows".to_string()
    })?;
    if path.contains(':') {
        return Err("路径含非法字符「:」".into());
    }
    let fetch = has_image_extension(path);
    let obj = path.replace('\\', "/"); // git 的对象路径一律正斜杠
    let (old_specs, new_specs) = match hash {
        Some(h) => {
            validate_hash(h)?;
            (vec![format!("{h}^:{obj}")], vec![format!("{h}:{obj}")])
        }
        None if staged => (vec![format!("HEAD:{obj}")], vec![format!(":{obj}")]),
        None => (vec![format!(":{obj}"), format!("HEAD:{obj}")], Vec::new()),
    };
    let old = read_object_side(dir, &old_specs, limit, fetch);
    let new = if new_specs.is_empty() {
        read_worktree_side(dir, path, limit, fetch)
    } else {
        read_object_side(dir, &new_specs, limit, fetch)
    };
    let image = old.iter().chain(new.iter()).any(|s| s.mime.is_some());
    Ok(BinaryPreview { path: path.to_string(), image, old, new })
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

fn log_result(
    result: Result<String, String>,
    in_work_tree: bool,
    head_exists: bool,
) -> Result<Vec<Commit>, String> {
    match result {
        Ok(text) => Ok(parse_log(&text)),
        // Unborn HEAD: inside a work tree, but HEAD points nowhere yet.
        Err(_) if in_work_tree && !head_exists => Ok(Vec::new()),
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
    match text {
        Ok(text) => Ok(parse_log(&text)),
        Err(e) => {
            let in_work_tree = run_git(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok();
            let head_exists = in_work_tree
                && run_git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok();
            log_result(Err(e), in_work_tree, head_exists)
        }
    }
}

/// 带筛选的 log：`query` → `--grep`、`author` → `--author`、`path` → 仅该文件历史。
/// 值以 `--grep=`/`--author=` 单参数形式传入（不经 shell，也不会被当成选项）。
pub fn git_log_filtered(
    dir: &Path,
    limit: u32,
    skip: u32,
    query: Option<&str>,
    author: Option<&str>,
    path: Option<&str>,
) -> Result<Vec<Commit>, String> {
    let limit = limit.min(200).to_string();
    let skip = skip.to_string();
    let grep = query.filter(|q| !q.trim().is_empty()).map(|q| format!("--grep={q}"));
    let auth = author.filter(|a| !a.trim().is_empty()).map(|a| format!("--author={a}"));

    let mut args: Vec<&str> = vec!["log", "--date-order", "-i", "--max-count", limit.as_str(), "--skip", skip.as_str()];
    if let Some(g) = grep.as_deref() {
        args.push(g);
    }
    if let Some(a) = auth.as_deref() {
        args.push(a);
    }
    args.push(LOG_FORMAT);
    if let Some(p) = path {
        args.push("--");
        args.push(p);
    }
    match run_git(dir, &args) {
        Ok(text) => Ok(parse_log(&text)),
        Err(e) => {
            let in_work_tree = run_git(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok();
            let head_exists = in_work_tree
                && run_git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok();
            log_result(Err(e), in_work_tree, head_exists)
        }
    }
}

const MAX_PATCH_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub hash: String,
    pub stat: String,
    pub additions: u32,
    pub deletions: u32,
    pub truncated: bool,
    pub files: Vec<FileDiff>,
}

pub fn validate_hash(hash: &str) -> Result<(), String> {
    let ok = (4..=40).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit());
    if ok {
        Ok(())
    } else {
        Err(format!("无效的提交哈希：{hash}"))
    }
}

pub fn truncate_patch(patch: String) -> (String, bool) {
    if patch.len() <= MAX_PATCH_BYTES {
        return (patch, false);
    }
    let mut end = MAX_PATCH_BYTES;
    while end > 0 && !patch.is_char_boundary(end) {
        end -= 1;
    }
    (patch[..end].to_string(), true)
}

pub fn commit_detail(dir: &Path, hash: &str) -> Result<CommitDetail, String> {
    validate_hash(hash)?;
    let stat = run_git(dir, &["show", "--stat", "--format=%H", hash, "--"])?;
    let raw = run_git(dir, &["show", "--format=", "--patch", hash, "--"])?;
    let (raw, truncated) = truncate_patch(raw);
    let files = parse_unified_diff(&raw);
    let additions = files.iter().map(|f| f.additions).sum();
    let deletions = files.iter().map(|f| f.deletions).sum();
    Ok(CommitDetail { hash: hash.to_string(), stat, additions, deletions, truncated, files })
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
    fn repo_status_defaults_to_not_truncated_and_serializes_the_flag() {
        let st = RepoStatus::not_repo("p1");
        assert!(!st.truncated);
        let json = serde_json::to_string(&st).unwrap();
        assert!(json.contains("\"truncated\":false"), "{json}");
    }

    /// 撞上 4MB 输出上限必须**如实上报**，而不是把残缺数据当完整结果用。
    #[test]
    fn run_git_capped_distinguishes_read_and_write_output_limits() {
        let Some(program) = resolve_git_path() else { return };
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let git = |args: &[&str]| {
            let out = Command::new(&program).arg("-C").arg(p).args(args).output().unwrap();
            assert!(out.status.success(), "git {args:?} failed");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.com"]);
        git(&["config", "user.name", "T"]);
        // 5MB 的提交信息 → `git log --format=%B` 的输出必然超过 4MB 上限
        let msg = p.join("msg.txt");
        std::fs::write(&msg, "x".repeat(5 * 1024 * 1024)).unwrap();
        let out = Command::new(&program)
            .arg("-C")
            .arg(p)
            .args(["commit", "--allow-empty", "-q", "-F"])
            .arg(&msg)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

        let small = run_git_capped(&program, p, &["status", "--porcelain=v2", "--branch"]).unwrap();
        assert!(!small.capped, "小输出不该被标记为截断");
        let big = run_git_capped(&program, p, &["log", "--format=%B"]).unwrap();
        assert!(big.capped, "5MB 输出必须被标记为截断");
        assert!(big.text.len() as u64 <= MAX_OUTPUT_BYTES);

        let err = run_git_with_opts(&program, p, &["log", "--format=%B"], GitRunOpts::default()).unwrap_err();
        assert!(err.contains("输出超过") && err.contains("已强制终止"), "{err}");
    }

    #[test]
    fn write_limit_and_timeout_errors_mark_result_uncertain() {
        let now = std::time::SystemTime::now();
        let capped = GitRaw {
            out: vec![b'x'; 8],
            err: Vec::new(),
            capped: true,
            exit_ok: false,
            timed_out: false,
            started_at: now,
        }
        .into_write_result(None)
        .unwrap_err();
        assert!(capped.contains("结果可能已部分生效"), "{capped}");

        let timed_out = GitRaw {
            out: Vec::new(),
            err: Vec::new(),
            capped: false,
            exit_ok: false,
            timed_out: true,
            started_at: now,
        }
        .into_write_result(None)
        .unwrap_err();
        assert!(timed_out.contains("结果可能已部分生效"), "{timed_out}");
    }

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        v.extend(b"IHDR");
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend([0u8; 16]);
        v
    }

    /// 像素预检是 H3 的核心防线：字节数小不代表解码后小。
    #[test]
    fn image_dimensions_matrix() {
        assert_eq!(image_dimensions(&png_header(800, 600)), Some((800, 600)));

        let mut gif = b"GIF89a".to_vec();
        gif.extend(320u16.to_le_bytes());
        gif.extend(240u16.to_le_bytes());
        assert_eq!(image_dimensions(&gif), Some((320, 240)));

        let mut bmp = b"BM".to_vec();
        bmp.extend([0u8; 16]);
        bmp.extend(100i32.to_le_bytes());
        bmp.extend((-50i32).to_le_bytes()); // 负高 = 自上而下
        assert_eq!(image_dimensions(&bmp), Some((100, 50)));

        // SOF0：ff c0, len=0x11, 精度 08, 高 0x012c=300, 宽 0x01f4=500
        let jpeg = vec![
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0x2c, 0x01, 0xf4, 0x03, 0x01, 0x11,
            0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00,
        ];
        assert_eq!(image_dimensions(&jpeg), Some((500, 300)));

        // ICO 里 0 表示 256
        assert_eq!(image_dimensions(&[0, 0, 1, 0, 1, 0, 0, 0]), Some((256, 256)));

        // 认不出来时不能瞎猜（此时只靠字节上限兜底）
        assert_eq!(image_dimensions(b"not an image at all"), None);
        assert_eq!(image_dimensions(&png_header(0, 600)), None);
    }

    #[test]
    fn make_side_drops_data_url_when_pixels_are_too_many() {
        // 9000×9000 = 8100 万像素，压缩后可能只有几百 KB，但解码是 324MB
        let big = png_header(9000, 9000);
        let side = make_side(big.len() as u64, MAX_PREVIEW_BYTES, Some(big));
        assert_eq!(side.mime.as_deref(), Some("image/png"));
        assert_eq!((side.width, side.height), (Some(9000), Some(9000)));
        assert!(side.over_pixels);
        assert!(side.too_big, "前端只看 too_big，两种超限都要算进来");
        assert!(side.data_url.is_none(), "超像素上限时绝不能回传 data URL");

        // 正常尺寸照常回传
        let ok = png_header(64, 64);
        let small = make_side(ok.len() as u64, MAX_PREVIEW_BYTES, Some(ok));
        assert!(!small.over_pixels);
        assert!(!small.too_big);
        assert!(small.data_url.is_some());
        assert_eq!((small.width, small.height), (Some(64), Some(64)));
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
        assert!(log_result(Err("fatal: bad default revision 'HEAD'".into()), true, false)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn git_log_real_error_propagates() {
        assert_eq!(
            log_result(Err("不是 git 仓库".to_string()), false, false).unwrap_err(),
            "不是 git 仓库"
        );
        assert_eq!(log_result(Err("fatal: bad revision".to_string()), true, true).unwrap_err(), "fatal: bad revision");
    }

    #[test]
    fn capped_result_truncates_instead_of_erroring() {
        let mut buf = vec![b'a'; (MAX_OUTPUT_BYTES + 1) as usize];
        buf[..3].copy_from_slice(b"abc");
        let got = capped_result(buf, true, false, false, "fatal: broken pipe", None).unwrap();
        assert_eq!(got.len(), MAX_OUTPUT_BYTES as usize);
        assert!(got.starts_with("abc"));
    }

    #[test]
    fn capped_result_passes_through_success_timeout_and_error() {
        assert_eq!(capped_result(b"ok".to_vec(), false, true, false, "", None).unwrap(), "ok");
        assert!(
            capped_result(Vec::new(), false, false, true, "", None).unwrap_err().contains("超时")
        );
        assert_eq!(
            capped_result(Vec::new(), false, false, false, "fatal: boom", None).unwrap_err(),
            "fatal: boom"
        );
    }

    #[test]
    fn timeout_error_carries_lock_cleanup_note() {
        let err = capped_result(Vec::new(), false, false, true, "", Some("已清理中断残留的 index.lock".into())).unwrap_err();
        assert!(err.contains("超时"), "{err}");
        assert!(err.contains("index.lock"), "{err}");
    }

    #[test]
    fn friendly_error_prefers_last_error_line_over_hook_noise() {
        let stderr = "fatal: needed a message\nerror: There was a problem with the pre-commit hook.\nhint: disable the hook\n";
        assert_eq!(friendly_git_error(stderr), "error: There was a problem with the pre-commit hook.");
        assert_eq!(friendly_git_error("some noise\nfatal: bad object"), "fatal: bad object");
        assert_eq!(friendly_git_error("plain words only"), "plain words only");
        assert_eq!(friendly_git_error(""), "git 执行失败");
    }

    #[test]
    fn repo_status_and_log_against_real_repo() {
        let program = match resolve_git_path() {
            Some(p) => p,
            None => return,
        };

        // Non-repo dir: not a repo, no error.
        let plain = tempfile::tempdir().unwrap();
        let st = repo_status("p", plain.path());
        assert!(!st.is_repo);
        assert!(st.error.is_none());

        // Fresh repo start (empty/unborn HEAD).
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let out = Command::new(&program)
                .current_dir(repo.path())
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q"]);
        assert!(git_log(repo.path(), 100, 0).unwrap().is_empty());
        assert!(repo_status("p", repo.path()).is_repo);

        // A real commit → is_repo with a branch and one log entry.
        git(&["config", "user.email", "devlaunch@example.com"]);
        git(&["config", "user.name", "DevLaunch Test"]);
        std::fs::write(repo.path().join("a.txt"), "hello").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-q", "-m", "init"]);
        let st = repo_status("p", repo.path());
        assert!(st.is_repo);
        assert!(st.branch.is_some());
        assert_eq!(git_log(repo.path(), 100, 0).unwrap().len(), 1);
    }

    #[test]
    fn sniffs_image_magic_and_rejects_lookalikes() {
        assert_eq!(sniff_image(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), Some("image/png"));
        assert_eq!(sniff_image(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"GIF89a...."), Some("image/gif"));
        assert_eq!(sniff_image(b"GIF87a...."), Some("image/gif"));
        assert_eq!(sniff_image(b"RIFF\x04\x00\x00\x00WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image(b"\x00\x00\x00\x1cftypavif"), Some("image/avif"));
        assert_eq!(sniff_image(&[0, 0, 1, 0, 1, 0]), Some("image/x-icon"));
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&14u32.to_le_bytes());
        bmp.extend_from_slice(&[0u8; 8]);
        assert_eq!(sniff_image(&bmp), Some("image/bmp"));
        // BMP/ICO 的魔数太弱：自洽性不成立就当二进制，否则任意文件都可能撞上
        let mut fake_bmp = b"BM".to_vec();
        fake_bmp.extend_from_slice(&4096u32.to_le_bytes());
        fake_bmp.extend_from_slice(&[0u8; 8]);
        assert_eq!(sniff_image(&fake_bmp), None);
        assert_eq!(sniff_image(&[0, 0, 1, 0, 0, 0]), None);
        assert_eq!(sniff_image(b"GIF88a...."), None);
        assert_eq!(sniff_image(b"just a plain text file"), None);
        assert_eq!(sniff_image(&[]), None);
    }

    #[test]
    fn image_extension_gate_decides_fetching_only() {
        assert!(has_image_extension("art/Player.PNG"));
        assert!(has_image_extension("icon.ico"));
        assert!(has_image_extension("a\\b\\c.jpeg"));
        assert!(!has_image_extension("README.md"));
        assert!(!has_image_extension("LICENSE"));
        assert!(!has_image_extension(".pngrc")); // 以点开头 ≠ 有扩展名
        assert!(!has_image_extension("scene.svg")); // SVG 是文本，本就走文本 diff
    }

    #[test]
    fn untracked_binary_file_is_not_rendered_as_text() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("blob.dat");
        std::fs::write(&bin, b"ab\x00cd").unwrap();
        let f = untracked_file_diff(dir.path(), "blob.dat").unwrap();
        assert!(f.binary, "含 NUL 的未跟踪文件不该按文本渲染");
        assert!(f.hunks.is_empty());

        let txt = dir.path().join("note.txt");
        std::fs::write(&txt, "line1\nline2").unwrap();
        let f = untracked_file_diff(dir.path(), "note.txt").unwrap();
        assert!(!f.binary);
        assert_eq!(f.hunks[0].lines.len(), 2);
    }

    /// 仓库自带图标：两张真实、尺寸不同的 PNG，正好当「同一张图片的两个版本」。
    fn fixture_pngs() -> Option<(Vec<u8>, Vec<u8>)> {
        let root = env!("CARGO_MANIFEST_DIR");
        let a = std::fs::read(format!("{root}/icons/32x32.png")).ok()?;
        let b = std::fs::read(format!("{root}/icons/128x128.png")).ok()?;
        Some((a, b))
    }

    #[test]
    fn binary_preview_reads_both_image_sides_verbatim() {
        let program = match resolve_git_path() {
            Some(p) => p,
            None => return,
        };
        let (old_png, new_png) = match fixture_pngs() {
            Some(p) => p,
            None => return,
        };
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let out = Command::new(&program).current_dir(repo.path()).args(args).output().unwrap();
            assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        let decode = |url: &str, mime: &str| -> Vec<u8> {
            use base64::{engine::general_purpose, Engine as _};
            let prefix = format!("data:{mime};base64,");
            assert!(url.starts_with(&prefix), "意外的 data URL 前缀：{}", &url[..prefix.len() + 8]);
            general_purpose::STANDARD.decode(&url[prefix.len()..]).unwrap()
        };

        git(&["init", "-q"]);
        git(&["config", "user.email", "devlaunch@example.com"]);
        git(&["config", "user.name", "DevLaunch Test"]);
        std::fs::create_dir(repo.path().join("art")).unwrap();
        let sprite = repo.path().join("art/player.png");
        std::fs::write(&sprite, &old_png).unwrap();
        git(&["add", "art/player.png"]);
        git(&["commit", "-q", "-m", "sprite"]);
        let first = git(&["rev-parse", "HEAD"]).trim().to_string();
        std::fs::write(&sprite, &new_png).unwrap(); // 第二版：只改工作区

        // 未暂存 = 索引（第一版）↔ 工作区磁盘（第二版）；字节必须原样往返，
        // 因为 run_git 的 lossy UTF-8 会改掉任何非文本字节。
        let p = binary_preview(repo.path(), "art/player.png", false, None, MAX_PREVIEW_BYTES).unwrap();
        assert!(p.image, "PNG 应按图片预览");
        let (o, n) = (p.old.as_ref().unwrap(), p.new.as_ref().unwrap());
        assert_eq!((o.size as usize, n.size as usize), (old_png.len(), new_png.len()));
        assert_eq!(o.mime.as_deref(), Some("image/png"));
        assert_eq!(decode(&o.data_url.clone().unwrap(), "image/png"), old_png);
        assert_eq!(decode(&n.data_url.clone().unwrap(), "image/png"), new_png);

        // 暂存 = HEAD ↔ 索引
        git(&["add", "art/player.png"]);
        let p = binary_preview(repo.path(), "art/player.png", true, None, MAX_PREVIEW_BYTES).unwrap();
        assert_eq!(decode(&p.new.as_ref().unwrap().data_url.clone().unwrap(), "image/png"), new_png);
        assert_eq!(decode(&p.old.as_ref().unwrap().data_url.clone().unwrap(), "image/png"), old_png);

        // History = 该提交 ↔ 第一父；首次提交无父 → 只剩新侧
        let p = binary_preview(repo.path(), "art/player.png", false, Some(&first), MAX_PREVIEW_BYTES).unwrap();
        assert!(p.old.is_none(), "首次提交没有父版本");
        assert_eq!(decode(&p.new.as_ref().unwrap().data_url.clone().unwrap(), "image/png"), old_png);

        // 超过上限：不读内容（也不去碰大对象），但字节数照报
        let p = binary_preview(repo.path(), "art/player.png", false, None, 16).unwrap();
        assert!(!p.image);
        assert!(p.old.as_ref().unwrap().too_big && p.new.as_ref().unwrap().too_big);
        assert!(p.old.as_ref().unwrap().data_url.is_none());

        // 非图片：扩展名不入围 → 不读内容，只有大小
        std::fs::write(repo.path().join("notes.txt"), "text").unwrap();
        git(&["add", "notes.txt"]);
        let p = binary_preview(repo.path(), "notes.txt", true, None, MAX_PREVIEW_BYTES).unwrap();
        assert!(!p.image);
        assert!(p.old.is_none());
        assert_eq!(p.new.as_ref().unwrap().size, 4);

        // 注入面：选项形状与 revspec 分隔符都必须被拒
        assert!(binary_preview(repo.path(), "art/player.png", false, Some("--all"), 1 << 20).is_err());
        assert!(binary_preview(repo.path(), "..:escape", false, None, 1 << 20).is_err());
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

    #[test]
    fn validate_hash_accepts_hex_and_rejects_options() {
        assert!(validate_hash("abc123").is_ok());
        assert!(validate_hash(&"a".repeat(40)).is_ok());
        assert!(validate_hash("--all").is_err());
        assert!(validate_hash("abc").is_err());
        assert!(validate_hash("gggg").is_err());
    }

    #[test]
    fn truncate_patch_marks_and_respects_utf8() {
        let small = "diff --git a b".to_string();
        assert_eq!(truncate_patch(small.clone()), (small, false));
        let big = "中".repeat(200_000); // >256KB bytes, multi-byte
        let (out, truncated) = truncate_patch(big);
        assert!(truncated);
        assert!(out.len() <= 256 * 1024);
        assert!(out.chars().all(|c| c == '中'));
    }

    #[test]
    fn parse_status_counts_conflicts() {
        let text = "u UU N... 100644 100644 100644 100644 a b c both.txt\n1 M. N... 100644 100644 100644 a b staged.txt\n";
        let st = parse_status("p1", text);
        assert_eq!(st.conflicts, 1);
        assert_eq!(st.staged, 2); // u + staged
        assert_eq!(st.unstaged, 1); // u
    }

    #[test]
    fn operation_from_git_dir_detects_markers() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(operation_from_git_dir(dir.path()), None);
        std::fs::write(dir.path().join("MERGE_HEAD"), "x").unwrap();
        assert_eq!(operation_from_git_dir(dir.path()), Some("merge".into()));
        std::fs::remove_file(dir.path().join("MERGE_HEAD")).unwrap();
        std::fs::create_dir(dir.path().join("rebase-merge")).unwrap();
        assert_eq!(operation_from_git_dir(dir.path()), Some("rebase".into()));
        std::fs::remove_dir(dir.path().join("rebase-merge")).unwrap();
        std::fs::write(dir.path().join("CHERRY_PICK_HEAD"), "x").unwrap();
        assert_eq!(operation_from_git_dir(dir.path()), Some("cherry-pick".into()));
    }

    #[test]
    fn parse_branches_reads_upstream_track() {
        let text = "main\x1forigin/main\x1f[ahead 2, behind 1]\nfeature\x1f\x1f\ndev\x1forigin/dev\x1f[gone]\n";
        let got = parse_branches(text, Some("main"));
        assert_eq!(got.len(), 3);
        let find = |n: &str| got.iter().find(|b| b.name == n).unwrap();
        let main = find("main");
        assert!(main.current);
        assert!(!main.remote);
        assert_eq!(main.upstream.as_deref(), Some("origin/main"));
        assert_eq!((main.ahead, main.behind), (2, 1));
        let feature = find("feature");
        assert_eq!(feature.upstream, None);
        assert!(!feature.current);
        let dev = find("dev");
        assert_eq!((dev.ahead, dev.behind), (0, 0));
    }

    #[test]
    fn parse_remote_branches_filters_head() {
        let got = parse_remote_branches("origin/main\u{1f}\norigin/HEAD\u{1f}refs/remotes/origin/main\nupstream/dev\u{1f}\n");
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|b| b.remote));
        assert!(got.iter().any(|b| b.name == "origin/main"));
        assert!(got.iter().all(|b| b.name != "origin/HEAD"));
    }

    #[test]
    fn parse_remote_branches_drops_bare_remote_head_ref() {
        // `git for-each-ref refs/remotes` 会列出 refs/remotes/origin（symref 非空），
        // 短名显示成 "origin"，选中它只会报「分支名为空」
        let got = parse_remote_branches("origin\u{1f}refs/remotes/origin/main\norigin/main\u{1f}\n");
        assert_eq!(got.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), vec!["origin/main"]);
        assert!(parse_remote_branches("").is_empty());
    }

    #[test]
    fn parse_branches_detached_has_no_current() {
        let text = "main\x1f\x1f\n";
        let got = parse_branches(text, None);
        assert!(got.iter().all(|b| !b.current));
    }

    #[test]
    fn untracked_file_diff_builds_add_hunks() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("new.txt");
        std::fs::write(&f, "alpha\nbeta\n").unwrap();
        let got = untracked_file_diff(dir.path(), "new.txt").unwrap();
        assert!(got.untracked);
        assert_eq!(got.additions, 2);
        assert_eq!(got.deletions, 0);
        assert_eq!(got.hunks.len(), 1);
        assert_eq!(got.hunks[0].lines[0].kind, "add");
        assert_eq!(got.hunks[0].lines[0].new_no, Some(1));
    }

    #[test]
    fn bounded_file_read_stops_at_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("large.bin");
        std::fs::write(&file, vec![b'x'; 4096]).unwrap();
        let (bytes, truncated) = read_file_limited(&file, 1024).unwrap();
        assert_eq!(bytes.len(), 1024);
        assert!(truncated);
    }

    #[test]
    fn repo_file_rejects_parent_paths() {
        let dir = tempfile::tempdir().unwrap();
        let err = repo_file_for_read(dir.path(), "../outside.txt").unwrap_err();
        assert!(err.contains("非法文件路径"), "{err}");
    }

    #[test]
    fn repo_file_accepts_file_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let file = root.join("inside.txt");
        std::fs::write(&file, "ok").unwrap();
        assert_eq!(repo_file_for_read(&root, "inside.txt").unwrap(), std::fs::canonicalize(file).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn repo_file_rejects_symlink_outside_root() {
        use std::os::windows::fs::symlink_file;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, "secret").unwrap();
        if symlink_file(&outside, root.join("link.txt")).is_err() {
            return;
        }
        let err = repo_file_for_read(&root, "link.txt").unwrap_err();
        assert!(err.contains("仓库外"), "{err}");
    }

    #[test]
    fn parse_unified_diff_multi_file_with_numbers() {
        let text = r#"diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,4 @@
 line1
-old2
+new2
+extra
 line3
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+alpha
+beta
"#;
        let files = parse_unified_diff(text);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].additions, 2);
        assert_eq!(files[0].deletions, 1);
        let lines = &files[0].hunks[0].lines;
        assert_eq!(lines[0].kind, "context");
        assert_eq!(lines[0].old_no, Some(1));
        assert_eq!(lines[0].new_no, Some(1));
        assert_eq!(lines[1].kind, "del");
        assert_eq!(lines[1].old_no, Some(2));
        assert_eq!(lines[1].new_no, None);
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[1].additions, 2);
        assert_eq!(files[1].deletions, 0);
        assert_eq!(files[1].hunks[0].lines[0].new_no, Some(1));
    }

    #[test]
    fn parse_unified_diff_binary_and_no_newline() {
        let bin = "diff --git a/x.bin b/x.bin\nBinary files a/x.bin and b/x.bin differ\n";
        let files = parse_unified_diff(bin);
        assert_eq!(files.len(), 1);
        assert!(files[0].binary);
        assert!(files[0].hunks.is_empty());

        let noeol = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n";
        let files2 = parse_unified_diff(noeol);
        assert_eq!(files2[0].hunks[0].lines.len(), 2);
    }

    #[test]
    fn resolve_git_effective_configured_wins_and_invalid_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("git.exe");
        std::fs::write(&good, "x").unwrap();

        let other = tempfile::tempdir().unwrap();
        let path_git = other.path().join("git.exe");
        std::fs::write(&path_git, "x").unwrap();
        let joined = std::env::join_paths([other.path()]).unwrap();

        // configured 有效：优先使用，即使 PATH 也有
        let got = resolve_git_effective(Some(good.to_str().unwrap()), None, Some(joined.as_os_str()), None);
        assert_eq!(got.as_deref(), Some(good.as_path()));

        // configured 无效：判定缺失，不回退到 PATH
        let missing = dir.path().join("nope.exe");
        assert_eq!(resolve_git_effective(Some(missing.to_str().unwrap()), None, Some(joined.as_os_str()), None), None);

        // configured 空 / None：回退 PATH
        assert_eq!(
            resolve_git_effective(Some("  "), None, Some(joined.as_os_str()), None).as_deref(),
            Some(path_git.as_path())
        );
        assert_eq!(
            resolve_git_effective(None, None, Some(joined.as_os_str()), None).as_deref(),
            Some(path_git.as_path())
        );
    }

    #[test]
    fn repo_status_without_git_reports_missing_not_non_repo() {
        let dir = tempfile::tempdir().unwrap();
        let st = repo_status_with("p1", dir.path(), None);
        assert!(!st.is_repo);
        assert!(st.error.unwrap().contains("未找到 git.exe"));
    }
}

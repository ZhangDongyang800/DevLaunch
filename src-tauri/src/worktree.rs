//! 按任务开发环境（git worktree）。
//!
//! 上半部是**纯函数**（解析 / 计算 / 校验），下半部是 git 与文件系统侧效应。
//! 写锁与配置落盘在 `commands.rs`，开窗在 `launcher.rs`。

use crate::config::{WorktreeLease, WorktreeSettings};
use crate::git::{run_git, run_git_opts, GitRunOpts};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 端口段上限：base + PORT_RANGE 内找不到空位就报错，不无限扩张。
pub const PORT_RANGE: u16 = 50;
pub const MAX_COPY_FILES: usize = 200;
/// 单个条目前最多遍历这么多文件就放弃（防 `**` 撞进巨型目录）。
pub const MAX_SCAN_FILES: usize = 2_000;
pub const MAX_COPY_FILE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_COPY_TOTAL_BYTES: u64 = 200 * 1024 * 1024;
/// `worktree add` 要 checkout 整棵树，5s 默认档不够。
const ADD_TIMEOUT: Duration = Duration::from_secs(60);
const REMOVE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    /// porcelain 输出里第一条即主工作区。
    pub is_main: bool,
    pub is_detached: bool,
    pub is_prunable: bool,
}

fn unquote(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }
        return out;
    }
    s.to_string()
}

fn branch_from_ref(reference: &str) -> Option<String> {
    reference.strip_prefix("refs/heads/").map(str::to_string)
}

pub fn parse_worktree_list(text: &str) -> Vec<WorktreeInfo> {
    let mut out: Vec<WorktreeInfo> = Vec::new();
    let mut cur: Option<WorktreeInfo> = None;
    for line in text.replace('\r', "").lines() {
        if let Some(value) = line.strip_prefix("worktree ") {
            if let Some(prev) = cur.take() {
                out.push(prev);
            }
            cur = Some(WorktreeInfo {
                path: unquote(value),
                head: String::new(),
                branch: None,
                is_main: out.is_empty(),
                is_detached: false,
                is_prunable: false,
            });
            continue;
        }
        let Some(entry) = cur.as_mut() else { continue };
        if let Some(value) = line.strip_prefix("HEAD ") {
            entry.head = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("branch ") {
            entry.branch = branch_from_ref(value.trim());
        } else if line == "detached" {
            entry.is_detached = true;
        } else if line.starts_with("prunable") {
            entry.is_prunable = true;
        }
    }
    if let Some(last) = cur {
        out.push(last);
    }
    out
}

fn normalize(path: &Path) -> PathBuf {
    path.components().collect()
}

/// 分支名 → 目录名。`validate_ref_name` 已排除空白与控制字符，这里再挡分隔符与点。
pub fn branch_slug(branch: &str) -> String {
    let replaced: String = branch
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() || r#"\/*?":<>|."#.contains(c) { '-' } else { c })
        .collect();
    let mut out = String::with_capacity(replaced.len());
    for c in replaced.chars() {
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.chars().count() > 48 {
        out = out.chars().take(48).collect();
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "wt".into()
    } else {
        out
    }
}

/// 环境根目录：配置缺失 → `<repo 同级>/<目录名>-wt`；相对 → 相对 rootDir；绝对 → 原样。
pub fn default_wt_root(root_dir: &Path, configured: Option<&str>) -> PathBuf {
    if let Some(raw) = configured.map(str::trim).filter(|s| !s.is_empty()) {
        let p = Path::new(raw);
        let joined = if p.is_absolute() { p.to_path_buf() } else { root_dir.join(p) };
        return normalize(&joined);
    }
    match (root_dir.parent(), root_dir.file_name().and_then(|s| s.to_str())) {
        (Some(parent), Some(name)) => normalize(&parent.join(format!("{name}{}", WorktreeSettings::DEFAULT_ROOT_SUFFIX))),
        _ => normalize(&root_dir.join(WorktreeSettings::DEFAULT_ROOT_SUFFIX.trim_start_matches('-'))),
    }
}

pub fn unique_path(root: &Path, slug: &str, exists: &dyn Fn(&Path) -> bool) -> Result<PathBuf, String> {
    let first = root.join(slug);
    if !exists(&first) {
        return Ok(first);
    }
    (2..=999)
        .map(|i| root.join(format!("{slug}-{i}")))
        .find(|c| !exists(c))
        // 回退到已存在的目录会让 git 报一句看不懂的错，这里直接说清楚。
        .ok_or_else(|| {
            format!(
                "{} 下 {slug}-2 … {slug}-999 全部被占用，请先清理环境根目录",
                root.display()
            )
        })
}

pub fn lease_port(leases: &[WorktreeLease], branch: &str) -> Option<u16> {
    let branch = branch.trim();
    leases.iter().find(|l| l.branch.trim() == branch).map(|l| l.port)
}

/// 同一分支的租约复用（幂等、稳定）；否则从 base 起找第一个未被其他分支占用的端口。
pub fn allocate_port(leases: &[WorktreeLease], branch: &str, base: u16) -> Result<u16, String> {
    if let Some(existing) = lease_port(leases, branch) {
        return Ok(existing);
    }
    let branch = branch.trim();
    let taken: BTreeSet<u16> = leases
        .iter()
        .filter(|l| l.branch.trim() != branch)
        .map(|l| l.port)
        .collect();
    for offset in 0..=PORT_RANGE {
        let Some(port) = base.checked_add(offset) else { break };
        if !taken.contains(&port) {
            return Ok(port);
        }
    }
    Err(format!(
        "端口段 {base}–{} 已用尽，请在环境设置里改 portBase",
        base.saturating_add(PORT_RANGE)
    ))
}

pub fn release_lease(leases: &[WorktreeLease], branch: &str) -> Vec<WorktreeLease> {
    let branch = branch.trim();
    leases.iter().filter(|l| l.branch.trim() != branch).cloned().collect()
}

/// 注入窗格的环境变量：端口（有租约才注入）+ 环境路径与分支。
pub fn pane_env(
    settings: &WorktreeSettings,
    branch: &str,
    path: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    if settings.port_base.is_some() {
        if let Some(port) = lease_port(&settings.leases, branch) {
            pairs.push((settings.effective_port_key().to_string(), port.to_string()));
        }
    }
    pairs.push(("DEVLAUNCH_WORKTREE".into(), path.to_string()));
    pairs.push(("DEVLAUNCH_WORKTREE_BRANCH".into(), branch.to_string()));
    for (k, v) in &pairs {
        validate_env_pair(k, v)?;
    }
    Ok(pairs)
}

pub fn validate_env_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("环境变量名为空".into());
    }
    let mut chars = key.chars();
    let ok = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!("非法环境变量名：{key}"))
    }
}

/// 值要同时穿过 cmd / PowerShell / bash 三条链路，取最严的字符集；不做「尽力转义」。
/// 括号允许：三条链路里值都在引号内（`set "K=V"` / `'…'`），而 `C:\Program Files (x86)\…`
/// 这类真实路径必须能作为环境路径注入。
pub fn validate_env_value(value: &str) -> Result<(), String> {
    if value.contains('\0') || value.chars().any(|c| c.is_control()) {
        return Err("环境变量值含控制字符".into());
    }
    if let Some(bad) = value.chars().find(|c| "\"%&|<>^$;`{}".contains(*c)) {
        return Err(format!("环境变量值含不支持的字符：{bad}"));
    }
    Ok(())
}

pub fn validate_env_pair(key: &str, value: &str) -> Result<(), String> {
    validate_env_key(key)?;
    validate_env_value(value)
}

/// 环境路径由后端从 `git worktree list` 解析，前端只传分支名。
pub fn resolve_worktree_path(list: &[WorktreeInfo], branch: &str) -> Result<PathBuf, String> {
    let branch = branch.trim();
    let entry = list
        .iter()
        .find(|w| w.branch.as_deref().map(str::trim) == Some(branch))
        .ok_or_else(|| format!("未找到环境：{branch}（可能已被删除）"))?;
    if entry.is_prunable {
        return Err(format!("环境 {branch} 的目录已不存在，请先在环境页清理失效记录"));
    }
    Ok(PathBuf::from(&entry.path))
}

pub fn normalize_rel(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

pub fn validate_copy_pattern(pattern: &str) -> Result<(), String> {
    let raw = pattern.trim();
    if raw.is_empty() {
        return Err("复制白名单条目为空".into());
    }
    if raw.contains('\0') || raw.chars().any(|c| c.is_control()) {
        return Err(format!("非法复制白名单条目：{pattern}"));
    }
    if raw.starts_with('/') || raw.contains(':') || Path::new(raw).is_absolute() {
        return Err(format!("复制白名单条目必须是仓库内相对路径：{pattern}"));
    }
    let normalized = normalize_rel(raw);
    if normalized.is_empty() {
        return Err(format!("复制白名单条目为空：{pattern}"));
    }
    for (i, seg) in segments(&normalized).iter().enumerate() {
        if *seg == ".." {
            return Err(format!("复制白名单条目不能跳出仓库：{pattern}"));
        }
        if *seg == ".git" {
            return Err(format!("不允许复制 .git 内容：{pattern}"));
        }
        if seg.contains("**") && i != segments(&normalized).len() - 1 {
            return Err(format!("** 只能出现在条目末尾：{pattern}"));
        }
        // seg_match 是多 * 的递归匹配，最坏情况随通配符数量指数增长；
        // 真实文件名模式用不到 3 个以上，超出的直接拒绝。
        if seg.chars().filter(|c| *c == '*' || *c == '?').count() > 3 {
            return Err(format!("单段通配符过多（最多 3 个）：{pattern}"));
        }
    }
    Ok(())
}

/// 条目里通配符之前的固定前缀；W2 用它决定 stat 还是遍历哪一层目录。
pub fn pattern_prefix(pattern: &str) -> String {
    let normalized = normalize_rel(pattern.trim());
    let mut out: Vec<&str> = Vec::new();
    for seg in segments(&normalized) {
        if seg.contains(['*', '?']) || seg == "**" {
            break;
        }
        out.push(seg);
    }
    out.join("/")
}

/// 单段通配：只支持 `*` 与 `?`，ASCII 大小写不敏感（Windows 文件系统语义）。
fn seg_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    fn helper(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => {
                let rest = &p[1..];
                (0..=t.len()).any(|i| helper(rest, &t[i..]))
            }
            Some('?') => !t.is_empty() && helper(&p[1..], &t[1..]),
            Some(c) => {
                !t.is_empty() && c.eq_ignore_ascii_case(&t[0]) && helper(&p[1..], &t[1..])
            }
        }
    }
    helper(&p, &t)
}

pub fn pattern_matches(pattern: &str, rel: &str) -> bool {
    let p = normalize_rel(pattern);
    let t = normalize_rel(rel);
    if p.is_empty() || t.is_empty() {
        return false;
    }
    let pseg = segments(&p);
    let tseg = segments(&t);
    if pseg.last().copied() == Some("**") {
        let prefix = &pseg[..pseg.len() - 1];
        return tseg.len() > prefix.len()
            && prefix.iter().zip(tseg.iter()).all(|(a, b)| seg_match(a, b));
    }
    pseg.len() == tseg.len() && pseg.iter().zip(tseg.iter()).all(|(a, b)| seg_match(a, b))
}

/// 永远不复制的东西：git 元数据、依赖与构建产物。
pub fn is_denied_copy_path(rel: &str) -> bool {
    let normalized = normalize_rel(rel);
    let seg = segments(&normalized);
    seg.iter().any(|s| *s == ".git" || *s == "node_modules")
        || matches!(seg.first().copied(), Some("target") | Some("dist"))
}

pub fn select_copy_files(candidates: &[String], patterns: &[String]) -> Vec<String> {
    let mut picked: Vec<String> = candidates
        .iter()
        .map(|c| normalize_rel(c))
        .filter(|c| !c.is_empty() && !is_denied_copy_path(c))
        .filter(|c| patterns.iter().any(|p| pattern_matches(p, c)))
        .collect();
    picked.sort();
    picked.dedup();
    picked
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOutcome {
    pub copied: usize,
    pub bytes: u64,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddOutcome {
    pub path: String,
    pub branch: String,
    pub port: Option<u16>,
    pub copied: usize,
    pub skipped: Vec<String>,
}

fn write_op(dir: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    run_git_opts(dir, args, GitRunOpts { allow_prompt: false, timeout, stdin_data: None })
}

pub fn list(dir: &Path) -> Result<Vec<WorktreeInfo>, String> {
    Ok(parse_worktree_list(&run_git(dir, &["worktree", "list", "--porcelain"])?))
}

fn branch_exists(dir: &Path, branch: &str) -> bool {
    let r = format!("refs/heads/{branch}");
    run_git(dir, &["rev-parse", "--verify", "--quiet", &r]).is_ok()
}

pub fn select_to_copy(src: &Path, patterns: &[String]) -> Result<Vec<String>, String> {
    for p in patterns {
        validate_copy_pattern(p)?;
    }
    let mut candidates: Vec<String> = Vec::new();
    for pattern in patterns {
        collect_candidates(src, &pattern_prefix(pattern), &mut candidates)?;
    }
    candidates.sort();
    candidates.dedup();
    Ok(select_copy_files(&candidates, patterns))
}

fn rel_of(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root).ok().map(|rel| normalize_rel(&rel.to_string_lossy()))
}

fn collect_candidates(root: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let base = if prefix.is_empty() { root.to_path_buf() } else { root.join(prefix) };
    let Ok(meta) = std::fs::symlink_metadata(&base) else { return Ok(()) };
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_file() {
        if let Some(rel) = rel_of(root, &base) {
            out.push(rel);
        }
        return Ok(());
    }
    let mut stack = vec![base];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            let Some(rel) = rel_of(root, &path) else { continue };
            if meta.file_type().is_symlink() || is_denied_copy_path(&rel) {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                if out.len() >= MAX_SCAN_FILES {
                    return Err(format!("复制白名单遍历超过 {MAX_SCAN_FILES} 个文件，请把条目写得更具体"));
                }
                out.push(rel);
            }
        }
    }
    Ok(())
}

/// 新建环境：先分端口 → checkout → 按 allow-list 复制。复制失败**不回滚**已建好的环境。
pub fn add(
    repo: &Path,
    settings: &WorktreeSettings,
    branch: &str,
    base: Option<&str>,
) -> Result<AddOutcome, String> {
    crate::git_write::validate_ref_name(branch)?;
    let branch = branch.trim();
    let base_ref = base.map(str::trim).filter(|s| !s.is_empty());
    if let Some(b) = base_ref {
        crate::git_write::validate_ref_name(b)?;
    }
    let existing = list(repo)?;
    if let Some(used) = existing.iter().find(|w| w.branch.as_deref().map(str::trim) == Some(branch)) {
        return Err(if used.is_main {
            format!("分支 {branch} 正在主工作区使用，请先在主工作区切到其他分支")
        } else {
            format!("分支 {branch} 已有环境：{}", used.path)
        });
    }
    // 端口在建目录前先分配：端口段用尽时不留下半成品环境。
    let port = settings.port_base.map(|base| allocate_port(&settings.leases, branch, base)).transpose()?;
    let root = default_wt_root(repo, settings.root.as_deref());
    std::fs::create_dir_all(&root).map_err(|e| format!("创建环境根目录失败：{e}"))?;
    let target = unique_path(&root, &branch_slug(branch), &|p| p.exists())?;
    let target_text = target.to_string_lossy().to_string();

    let owned: Vec<String> = if branch_exists(repo, branch) {
        vec![target_text.clone(), branch.to_string()]
    } else {
        vec![
            "-b".to_string(),
            branch.to_string(),
            target_text.clone(),
            base_ref.unwrap_or("HEAD").to_string(),
        ]
    };
    let mut args = vec!["worktree", "add"];
    args.extend(owned.iter().map(String::as_str));
    write_op(repo, &args, ADD_TIMEOUT).map_err(|e| format!("创建环境失败：{e}"))?;

    let (copied, skipped) = if settings.copy.is_empty() {
        (0, Vec::new())
    } else {
        let files = select_to_copy(repo, &settings.copy)?;
        let out = perform_copy(repo, &target, &files)?;
        (out.copied, out.skipped)
    };
    Ok(AddOutcome { path: target_text, branch: branch.to_string(), port, copied, skipped })
}

pub fn remove(repo: &Path, branch: &str, force: bool) -> Result<(), String> {
    crate::git_write::validate_ref_name(branch)?;
    let branch = branch.trim();
    let existing = list(repo)?;
    let entry = existing
        .iter()
        .find(|w| w.branch.as_deref().map(str::trim) == Some(branch))
        .ok_or_else(|| format!("未找到环境：{branch}（可能已被删除）"))?;
    if entry.is_main {
        return Err("不能删除主工作区".into());
    }
    if entry.is_prunable {
        return Err(format!("环境 {branch} 的目录已不存在，请改用「清理失效记录」"));
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(entry.path.as_str());
    write_op(repo, &args, REMOVE_TIMEOUT).map_err(|e| {
        if !force && e.contains("modified or untracked files") {
            "该环境有未提交改动；确认全部丢弃请勾选「强制删除」".to_string()
        } else {
            e
        }
    })?;
    Ok(())
}

/// 只清理失效记录（目录已被手工删除的条目），不碰任何存在的工作树。
pub fn prune(repo: &Path) -> Result<String, String> {
    write_op(repo, &["worktree", "prune", "--verbose"], REMOVE_TIMEOUT)
}

/// 把白名单文件从源工作区复制到新环境。**默认不覆盖**已存在的目标文件。
pub fn perform_copy(src: &Path, dst: &Path, files: &[String]) -> Result<CopyOutcome, String> {
    if files.len() > MAX_COPY_FILES {
        return Err(format!("复制白名单命中 {} 个文件，超过 {MAX_COPY_FILES} 上限", files.len()));
    }
    let mut out = CopyOutcome::default();
    let mut declared: Vec<(String, PathBuf, PathBuf, u64)> = Vec::with_capacity(files.len());
    for rel in files {
        let source = src.join(normalize_rel(rel));
        let meta = std::fs::symlink_metadata(&source)
            .map_err(|e| format!("读取 {rel} 失败：{e}"))?;
        if meta.file_type().is_symlink() {
            out.skipped.push(rel.clone());
            continue;
        }
        if !meta.is_file() {
            return Err(format!("{rel} 不是文件，无法复制"));
        }
        if meta.len() > MAX_COPY_FILE_BYTES {
            return Err(format!("{rel} 超过单文件 20MB 上限，未执行复制"));
        }
        declared.push((rel.clone(), source, dst.join(normalize_rel(rel)), meta.len()));
    }
    let total: u64 = declared.iter().map(|d| d.3).sum();
    if total > MAX_COPY_TOTAL_BYTES {
        return Err("白名单合计超过 200MB，未执行复制".into());
    }
    for (rel, source, target, bytes) in declared {
        if target.exists() {
            out.skipped.push(rel);
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
        }
        std::fs::copy(&source, &target).map_err(|e| format!("复制 {rel} 失败：{e}"))?;
        out.copied += 1;
        out.bytes += bytes;
    }
    Ok(out)
}

/// 删除环境后回收租约并写回配置；端口不再被复用以外的逻辑依赖。
pub fn upsert_lease(leases: &mut Vec<WorktreeLease>, branch: &str, port: u16) {
    let branch = branch.trim().to_string();
    match leases.iter_mut().find(|l| l.branch.trim() == branch.as_str()) {
        Some(existing) => existing.port = port,
        None => leases.push(WorktreeLease { branch, port }),
    }
    leases.sort_by(|a, b| a.branch.cmp(&b.branch));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(branch: &str, port: u16) -> WorktreeLease {
        WorktreeLease { branch: branch.into(), port }
    }

    const SAMPLE: &str = "worktree D:/Projects/repo\nHEAD 1111111111111111111111111111111111111111\nbranch refs/heads/main\n\n\
        worktree D:/Projects/repo-wt/feature-cart\nHEAD 2222222222222222222222222222222222222222\nbranch refs/heads/feature/cart\n\n";

    #[test]
    fn parse_worktree_list_marks_main_and_branches() {
        let got = parse_worktree_list(SAMPLE);
        assert_eq!(got.len(), 2);
        assert!(got[0].is_main && !got[1].is_main);
        assert_eq!(got[0].path, "D:/Projects/repo");
        assert_eq!(got[1].path, "D:/Projects/repo-wt/feature-cart");
        assert_eq!(got[1].branch.as_deref(), Some("feature/cart"));
        assert_eq!(got[0].head, "1".repeat(40));
        assert!(!got[1].is_prunable && !got[1].is_detached);
    }

    #[test]
    fn parse_worktree_list_handles_detached_prunable_and_quoted_paths() {
        let text = "worktree \"D:/My Proj/a\\bb\"\nHEAD 333\ndetached\n\n\
            worktree D:/gone\nHEAD 444\nbranch refs/heads/x\nprunable gitdir file points to non-existent location\n";
        let got = parse_worktree_list(text);
        assert_eq!(got[0].path, r"D:/My Proj/a\bb");
        assert!(got[0].is_detached);
        assert_eq!(got[0].branch, None);
        assert!(got[1].is_prunable);
        assert_eq!(got[1].branch.as_deref(), Some("x"));
    }

    #[test]
    fn parse_worktree_list_ignores_foreign_refs() {
        let got = parse_worktree_list("worktree D:/r\nHEAD 555\nbranch refs/remotes/origin/main\n");
        assert_eq!(got[0].branch, None);
    }

    #[test]
    fn branch_slug_matrix() {
        assert_eq!(branch_slug("feature/cart"), "feature-cart");
        assert_eq!(branch_slug("fix\\a:b*c?d\"e<f>g|h."), "fix-a-b-c-d-e-f-g-h");
        assert_eq!(branch_slug("main"), "main");
        assert_eq!(branch_slug("feat/中文/任务"), "feat-中文-任务");
        assert_eq!(branch_slug(".."), "wt");
        assert_eq!(branch_slug("a//b"), "a-b");
        let long = branch_slug(&"x".repeat(200));
        assert_eq!(long.chars().count(), 48);
    }

    #[test]
    fn default_wt_root_matrix() {
        let root = Path::new(r"D:\Projects\repo");
        assert_eq!(default_wt_root(root, None), PathBuf::from(r"D:\Projects\repo-wt"));
        assert_eq!(default_wt_root(root, Some(".wt")), PathBuf::from(r"D:\Projects\repo\.wt"));
        assert_eq!(
            default_wt_root(root, Some(r"E:\wt\dir\")),
            PathBuf::from(r"E:\wt\dir")
        );
        assert_eq!(default_wt_root(root, Some("   ")), PathBuf::from(r"D:\Projects\repo-wt"));
        // 盘根没有父目录可借用 → 退回仓库内
        assert_eq!(default_wt_root(Path::new(r"D:\"), None), PathBuf::from(r"D:\wt"));
    }

    #[test]
    fn unique_path_avoids_existing_directories() {
        let taken = std::collections::HashSet::from([
            PathBuf::from(r"D:\wt\feat"),
            PathBuf::from(r"D:\wt\feat-2"),
        ]);
        let got = unique_path(Path::new(r"D:\wt"), "feat", &|p| taken.contains(p)).unwrap();
        assert_eq!(got, PathBuf::from(r"D:\wt\feat-3"));
    }

    #[test]
    fn unique_path_errors_when_all_names_taken() {
        let got = unique_path(Path::new(r"D:\wt"), "feat", &|_| true).unwrap_err();
        assert!(got.contains("全部被占用"), "{got}");
    }

    #[test]
    fn allocate_port_is_stable_and_skips_taken() {
        let leases = vec![lease("a", 5173), lease("b", 5174)];
        assert_eq!(allocate_port(&leases, "a", 5173).unwrap(), 5173);
        assert_eq!(allocate_port(&leases, "c", 5173).unwrap(), 5175);
        assert_eq!(allocate_port(&[], "c", 5173).unwrap(), 5173);
    }

    #[test]
    fn allocate_port_errors_when_range_exhausted() {
        let leases: Vec<WorktreeLease> =
            (0..=PORT_RANGE).map(|i| lease(&format!("b{i}"), 3000 + i)).collect();
        let err = allocate_port(&leases, "new", 3000).unwrap_err();
        assert!(err.contains("端口段") && err.contains("portBase"), "{err}");
    }

    #[test]
    fn lease_upsert_and_release() {
        let mut leases = vec![lease("b", 3001), lease("a", 3000)];
        upsert_lease(&mut leases, "c", 3002);
        assert_eq!(
            leases.iter().map(|l| l.branch.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        upsert_lease(&mut leases, "a", 3050);
        assert_eq!(lease_port(&leases, "a"), Some(3050));
        let after = release_lease(&leases, "b");
        assert_eq!(after.len(), 2);
        assert_eq!(lease_port(&after, "b"), None);
    }

    #[test]
    fn validate_env_pair_matrix() {
        assert!(validate_env_pair("PORT", "5173").is_ok());
        assert!(validate_env_pair("DEVLAUNCH_WORKTREE", r"D:\My Proj\wt 中文").is_ok());
        // 括号在三种 shell 里都落在引号内，Program Files (x86) 这类路径必须可用
        assert!(validate_env_pair("DEVLAUNCH_WORKTREE", r"C:\Program Files (x86)\wt").is_ok());
        assert!(validate_env_pair("_A9", "x").is_ok());
        assert!(validate_env_pair("9PORT", "x").is_err());
        assert!(validate_env_pair("A B", "x").is_err());
        assert!(validate_env_pair("", "x").is_err());
        for bad in ["a%b", "a\"b", "a&b", "a|b", "a<b", "a>b", "a^b", "a$b", "a;b", "a`b", "a{b"] {
            assert!(validate_env_pair("PORT", bad).is_err(), "value {bad}");
        }
        assert!(validate_env_pair("PORT", "5173\nrm -rf").is_err());
        assert!(validate_env_pair("PORT", "it's ok").is_ok());
    }

    #[test]
    fn pane_env_only_injects_port_with_lease_and_validates() {
        let settings = WorktreeSettings {
            port_base: Some(5173),
            port_key: Some("WEB_PORT".into()),
            leases: vec![lease("feat", 5175)],
            ..Default::default()
        };
        let env = pane_env(&settings, "feat", r"D:\repo-wt\feat").unwrap();
        assert_eq!(
            env,
            vec![
                ("WEB_PORT".to_string(), "5175".to_string()),
                ("DEVLAUNCH_WORKTREE".to_string(), r"D:\repo-wt\feat".to_string()),
                ("DEVLAUNCH_WORKTREE_BRANCH".to_string(), "feat".to_string()),
            ]
        );
        // 没有租约（例如在设置 portBase 之前建的环境）→ 不注入端口
        let no_lease = pane_env(&settings, "other", r"D:\repo-wt\other").unwrap();
        assert_eq!(no_lease.len(), 2);
        // 未配置 portBase → 一律不注入端口
        let mut off = settings.clone();
        off.port_base = None;
        assert!(!pane_env(&off, "feat", "D:/x").unwrap().iter().any(|(k, _)| k == "WEB_PORT"));
        // 值非法（含 %）→ 拒绝而不是硬塞
        assert!(pane_env(&settings, "feat", r"D:\x%y").is_err());
    }

    #[test]
    fn resolve_worktree_path_only_accepts_known_live_branches() {
        let list = parse_worktree_list(SAMPLE);
        assert_eq!(
            resolve_worktree_path(&list, "feature/cart").unwrap(),
            PathBuf::from("D:/Projects/repo-wt/feature-cart")
        );
        assert!(resolve_worktree_path(&list, "nope").unwrap_err().contains("未找到环境"));
        let prunable = parse_worktree_list("worktree D:/gone\nHEAD 1\nbranch refs/heads/g\nprunable x\n");
        assert!(resolve_worktree_path(&prunable, "g").unwrap_err().contains("清理失效记录"));
    }

    #[test]
    fn pattern_matches_matrix() {
        assert!(pattern_matches(".env", ".env"));
        assert!(pattern_matches(".ENV", ".env"));
        assert!(pattern_matches(r"config\local.yml", "config/local.yml"));
        assert!(pattern_matches("config/*.yml", "config/local.yml"));
        assert!(!pattern_matches("config/*.yml", "config/sub/local.yml"));
        assert!(!pattern_matches("config/*.yml", "config/local.js"));
        assert!(pattern_matches("seeds/**", "seeds/a.json"));
        assert!(pattern_matches("seeds/**", "seeds/deep/a.json"));
        assert!(!pattern_matches("seeds/**", "seeds"));
        assert!(pattern_matches("local-?", "local-1"));
        assert!(!pattern_matches("local-?", "local-12"));
        assert!(!pattern_matches(".env", ".env.local"));
        assert!(!pattern_matches("", "a"));
    }

    #[test]
    fn validate_copy_pattern_rejects_escapes_and_git() {
        assert!(validate_copy_pattern(".env").is_ok());
        assert!(validate_copy_pattern(r"config\local.*").is_ok());
        assert!(validate_copy_pattern("seeds/**").is_ok());
        assert!(validate_copy_pattern("").is_err());
        assert!(validate_copy_pattern("../secrets").is_err());
        assert!(validate_copy_pattern("config/../../x").is_err());
        assert!(validate_copy_pattern("/etc/passwd").is_err());
        assert!(validate_copy_pattern(r"C:\x\.env").is_err());
        assert!(validate_copy_pattern(".git/config").is_err());
        assert!(validate_copy_pattern("**/secret").is_err());
        assert!(validate_copy_pattern("a\0b").is_err());
        assert!(validate_copy_pattern("a*b*c*d*e").is_err());
        assert!(validate_copy_pattern("a*b*c").is_ok());
    }

    #[test]
    fn pattern_prefix_stops_at_first_wildcard() {
        assert_eq!(pattern_prefix(".env"), ".env");
        assert_eq!(pattern_prefix(r"config\local.*"), "config");
        assert_eq!(pattern_prefix("seeds/**"), "seeds");
        assert_eq!(pattern_prefix("*.txt"), "");
        assert_eq!(pattern_prefix(r"a\b\c.yml"), "a/b/c.yml");
    }

    #[test]
    fn select_copy_files_matches_denies_and_dedups() {
        let candidates = vec![
            ".env".to_string(),
            "./.env".to_string(),
            r"config\local.yml".to_string(),
            ".git/config".to_string(),
            "node_modules/pkg/.env".to_string(),
            "target/debug/.env".to_string(),
            "dist/.env".to_string(),
        ];
        let patterns = vec![".env".to_string(), "config/*.yml".to_string()];
        assert_eq!(
            select_copy_files(&candidates, &patterns),
            vec![".env".to_string(), "config/local.yml".to_string()]
        );
        assert!(select_copy_files(&candidates, &[]).is_empty());
    }

    #[test]
    fn perform_copy_creates_parents_and_never_overwrites() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join("config")).unwrap();
        std::fs::write(src.path().join(".env"), "A=1").unwrap();
        std::fs::write(src.path().join("config/local.yml"), "b: 2").unwrap();
        std::fs::create_dir_all(dst.path().join("nested")).unwrap();
        std::fs::create_dir_all(src.path().join("nested")).unwrap();
        std::fs::write(src.path().join("nested/.env"), "source").unwrap();
        std::fs::write(dst.path().join("nested/.env"), "existing").unwrap();

        let files = vec![".env".to_string(), "config/local.yml".to_string(), "nested/.env".to_string()];
        let out = perform_copy(src.path(), dst.path(), &files).unwrap();
        assert_eq!(out.copied, 2);
        assert_eq!(out.bytes, 7);
        assert_eq!(out.skipped, vec!["nested/.env".to_string()]);
        assert_eq!(std::fs::read_to_string(dst.path().join("config/local.yml")).unwrap(), "b: 2");
        assert_eq!(std::fs::read_to_string(dst.path().join(".env")).unwrap(), "A=1");
        assert_eq!(std::fs::read_to_string(dst.path().join("nested/.env")).unwrap(), "existing");
    }

    #[test]
    fn perform_copy_errors_on_missing_source() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let err = perform_copy(src.path(), dst.path(), &["gone.env".into()]).unwrap_err();
        assert!(err.contains("读取 gone.env 失败"), "{err}");
        assert!(dst.path().read_dir().unwrap().next().is_none());
    }

    fn have_git() -> bool {
        crate::git::resolve_git_path().is_some()
    }

    fn git_git(dir: &Path, args: &[&str]) {
        let program = crate::git::resolve_git_path().unwrap();
        let out = std::process::Command::new(program).arg("-C").arg(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// 真仓库 + 一个未跟踪的 .env + 一个跟踪的 config/local.yml。
    fn repo_with_env() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git_git(dir.path(), &["init", "-q"]);
        git_git(dir.path(), &["config", "user.email", "t@e.com"]);
        git_git(dir.path(), &["config", "user.name", "T"]);
        std::fs::create_dir(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        std::fs::write(dir.path().join("config/local.yml"), "b: 2\n").unwrap();
        git_git(dir.path(), &["add", "a.txt", "config/local.yml"]);
        git_git(dir.path(), &["commit", "-q", "-m", "init"]);
        std::fs::write(dir.path().join(".env"), "SECRET=1\n").unwrap();
        dir
    }

    fn settings() -> WorktreeSettings {
        WorktreeSettings {
            root: Some(".wt".into()),
            copy: vec![".env".into(), "config/*.yml".into()],
            port_base: Some(4000),
            port_key: None,
            leases: vec![],
        }
    }

    #[test]
    fn add_list_remove_against_real_repo() {
        if !have_git() {
            return;
        }
        let repo = repo_with_env();
        let created = add(repo.path(), &settings(), "feat/cart", None).unwrap();
        assert_eq!(created.branch, "feat/cart");
        assert_eq!(created.port, Some(4000));
        // 白名单里**已提交**的文件在新环境里必然已存在 → 跳过不覆盖（白名单实际只对未跟踪/本地文件生效）
        assert_eq!(created.copied, 1, "skipped={:?}", created.skipped);
        assert_eq!(created.skipped, vec!["config/local.yml".to_string()]);
        let worktree = PathBuf::from(&created.path);
        assert!(worktree.join("a.txt").is_file());
        assert_eq!(std::fs::read_to_string(worktree.join(".env")).unwrap(), "SECRET=1\n");
        assert!(worktree.join("config/local.yml").is_file());

        let listed = list(repo.path()).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].is_main && !listed[1].is_main);
        assert_eq!(listed[1].branch.as_deref(), Some("feat/cart"));
        assert!(!listed[1].is_prunable);

        // 同一分支不能开第二个环境
        assert!(add(repo.path(), &settings(), "feat/cart", None).unwrap_err().contains("已有环境"));

        remove(repo.path(), "feat/cart", true).unwrap();
        assert!(!worktree.exists());
        assert_eq!(list(repo.path()).unwrap().len(), 1);
    }

    #[test]
    fn remove_protects_main_and_requires_force_when_dirty() {
        if !have_git() {
            return;
        }
        let repo = repo_with_env();
        let main_branch = list(repo.path()).unwrap().remove(0).branch.unwrap();
        assert!(remove(repo.path(), &main_branch, true).unwrap_err().contains("主工作区"));

        let created = add(repo.path(), &settings(), "dirty", None).unwrap();
        std::fs::write(Path::new(&created.path).join("junk.txt"), "x").unwrap();
        let err = remove(repo.path(), "dirty", false).unwrap_err();
        assert!(err.contains("强制删除"), "{err}");
        assert!(Path::new(&created.path).exists());
        remove(repo.path(), "dirty", true).unwrap();
        assert!(!Path::new(&created.path).exists());
    }

    #[test]
    fn second_environment_gets_next_port_and_reuses_lease() {
        if !have_git() {
            return;
        }
        let repo = repo_with_env();
        let mut live = settings();
        let first = add(repo.path(), &live, "a-one", None).unwrap();
        upsert_lease(&mut live.leases, "a-one", first.port.unwrap());
        let second = add(repo.path(), &live, "a-two", None).unwrap();
        assert_eq!((first.port, second.port), (Some(4000), Some(4001)));
        // 租约已存在时复用同端口
        let again = add(repo.path(), &live, "a-one", None);
        assert!(again.unwrap_err().contains("已有环境"));
        remove(repo.path(), "a-one", true).unwrap();
        remove(repo.path(), "a-two", true).unwrap();
    }

    #[test]
    fn prune_clears_records_of_manually_deleted_dirs() {
        if !have_git() {
            return;
        }
        let repo = repo_with_env();
        let created = add(repo.path(), &settings(), "gone-soon", None).unwrap();
        std::fs::remove_dir_all(&created.path).unwrap();
        let listed = list(repo.path()).unwrap();
        assert!(listed.iter().any(|w| w.branch.as_deref() == Some("gone-soon") && w.is_prunable));
        assert!(remove(repo.path(), "gone-soon", true).unwrap_err().contains("清理失效记录"));
        prune(repo.path()).unwrap();
        assert_eq!(list(repo.path()).unwrap().len(), 1);
    }

    #[test]
    fn select_to_copy_walks_real_tree_and_drops_denied_paths() {
        let repo = repo_with_env();
        std::fs::create_dir_all(repo.path().join("node_modules/pkg")).unwrap();
        std::fs::write(repo.path().join("node_modules/pkg/.env"), "x").unwrap();
        std::fs::create_dir_all(repo.path().join("target/debug")).unwrap();
        std::fs::write(repo.path().join("target/debug/.env"), "x").unwrap();
        let patterns = vec![
            ".env".into(),
            "config/*.yml".into(),
            "node_modules/**".into(),
            "target/**".into(),
            "missing.env".into(),
        ];
        assert_eq!(
            select_to_copy(repo.path(), &patterns).unwrap(),
            vec![".env".to_string(), "config/local.yml".to_string()]
        );
        assert!(select_to_copy(repo.path(), &[".git/config".into()]).is_err());
        assert!(select_to_copy(repo.path(), &["../escape".into()]).is_err());
    }
}

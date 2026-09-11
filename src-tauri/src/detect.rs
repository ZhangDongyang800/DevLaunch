use crate::config::Shell;
use serde::Serialize;
use std::fs;
use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_TOTAL_SUGGESTIONS: usize = 8;
pub const MAX_VISITED_DIRS: usize = 2000;
pub const MAX_SCAN_RESULTS: usize = 200;

const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "target", "dist", "build", "out", "bin", "obj",
    ".venv", "venv", "__pycache__", ".idea", ".vscode", ".next",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub name: String,
    pub work_dir: Option<String>,
    pub shell: Shell,
    pub command: String,
    pub ecosystem: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    pub ecosystems: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

pub fn detect(root: &Path) -> DetectResult {
    let mut suggestions: Vec<Suggestion> = Vec::new();
    collect_dir(root, None, &mut suggestions);
    if suggestions.len() < MAX_TOTAL_SUGGESTIONS {
        for name in visible_subdirs(root) {
            collect_dir(&root.join(&name), Some(&name), &mut suggestions);
            if suggestions.len() >= MAX_TOTAL_SUGGESTIONS {
                break;
            }
        }
    }
    suggestions.truncate(MAX_TOTAL_SUGGESTIONS);
    let ecosystems = unique_ecosystems(&suggestions);
    DetectResult { ecosystems, suggestions }
}

fn collect_dir(dir: &Path, subdir: Option<&str>, out: &mut Vec<Suggestion>) {
    if dir.join("package.json").is_file() {
        out.extend(node_suggestions(dir, subdir));
    }
    if dir.join("Cargo.toml").is_file() {
        out.extend(rust_suggestions(dir, subdir));
    }
    if dir.join("go.mod").is_file() {
        out.extend(go_suggestions(dir, subdir));
    }
    out.extend(python_suggestions(dir, subdir));
}

fn visible_subdirs(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = fs::read_dir(root) else { return names };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        // file_type() 不跟随符号链接：symlink 目录的 is_dir() 为 false，自然跳过
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            names.push(name);
        }
    }
    names.sort();
    names
}

fn read_text(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > MAX_FILE_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&read_text(path)?).ok()
}

fn node_suggestions(dir: &Path, subdir: Option<&str>) -> Vec<Suggestion> {
    let Some(json) = read_json(&dir.join("package.json")) else { return Vec::new() };
    let Some(scripts) = json.get("scripts").and_then(|v| v.as_object()) else { return Vec::new() };
    let pm = package_manager(dir);
    let mut picked: Vec<&str> = Vec::new();
    for script in ["dev", "start", "serve", "preview"] {
        if scripts.contains_key(script) {
            picked.push(script);
        }
        if picked.len() == 2 {
            break;
        }
    }
    picked
        .iter()
        .enumerate()
        .map(|(i, script)| Suggestion {
            name: match subdir {
                None => (*script).to_string(),
                Some(d) if i == 0 => d.to_string(),
                Some(d) => format!("{d}: {script}"),
            },
            work_dir: subdir.map(str::to_string),
            shell: Shell::Cmd,
            command: format!("{pm} run {script}"),
            ecosystem: "node".to_string(),
        })
        .collect()
}

fn package_manager(dir: &Path) -> &'static str {
    if dir.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if dir.join("yarn.lock").is_file() {
        "yarn"
    } else if dir.join("bun.lockb").is_file() || dir.join("bun.lock").is_file() {
        "bun"
    } else {
        "npm"
    }
}

fn item_name(subdir: Option<&str>, fallback: &str) -> String {
    match subdir {
        Some(d) => d.to_string(),
        None => fallback.to_string(),
    }
}

fn rust_suggestions(dir: &Path, subdir: Option<&str>) -> Vec<Suggestion> {
    let Some(text) = read_text(&dir.join("Cargo.toml")) else { return Vec::new() };
    if !text.lines().any(|l| l.trim() == "[package]") {
        return Vec::new();
    }
    vec![Suggestion {
        name: item_name(subdir, "run"),
        work_dir: subdir.map(str::to_string),
        shell: Shell::Cmd,
        command: "cargo run".to_string(),
        ecosystem: "rust".to_string(),
    }]
}

fn go_suggestions(dir: &Path, subdir: Option<&str>) -> Vec<Suggestion> {
    let command = if dir.join("main.go").is_file() {
        "go run .".to_string()
    } else {
        let mut names: Vec<String> = Vec::new();
        if let Ok(entries) = fs::read_dir(dir.join("cmd")) {
            for entry in entries.flatten() {
                if entry.path().join("main.go").is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names.sort();
        let Some(first) = names.into_iter().next() else { return Vec::new() };
        format!("go run ./cmd/{first}")
    };
    vec![Suggestion {
        name: item_name(subdir, "run"),
        work_dir: subdir.map(str::to_string),
        shell: Shell::Cmd,
        command,
        ecosystem: "go".to_string(),
    }]
}

fn python_suggestions(dir: &Path, subdir: Option<&str>) -> Vec<Suggestion> {
    let candidates = [
        ("manage.py", "python manage.py runserver", "manage"),
        ("main.py", "python main.py", "main"),
        ("app.py", "python app.py", "app"),
    ];
    for (file, command, stem) in candidates {
        if dir.join(file).is_file() {
            return vec![Suggestion {
                name: item_name(subdir, stem),
                work_dir: subdir.map(str::to_string),
                shell: Shell::Cmd,
                command: command.to_string(),
                ecosystem: "python".to_string(),
            }];
        }
    }
    Vec::new()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScannedRepo {
    pub name: String,
    pub root_dir: String,
    pub ecosystems: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

pub fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

pub fn scan(root: &Path) -> Vec<ScannedRepo> {
    let mut repos = collect_scan(root);
    repos.sort_by_key(|r| r.root_dir.to_ascii_lowercase());
    repos
}

fn collect_scan(root: &Path) -> Vec<ScannedRepo> {
    if is_git_repo(root) {
        return vec![scanned_repo(root)];
    }
    let mut repos: Vec<ScannedRepo> = Vec::new();
    let mut visited = 0usize;
    let mut level1: Vec<std::path::PathBuf> = Vec::new();
    for name in visible_subdirs(root) {
        visited += 1;
        if visited > MAX_VISITED_DIRS || repos.len() >= MAX_SCAN_RESULTS {
            return repos;
        }
        let p = root.join(&name);
        if is_git_repo(&p) {
            repos.push(scanned_repo(&p));
        } else {
            level1.push(p);
        }
    }
    for parent in level1 {
        for name in visible_subdirs(&parent) {
            visited += 1;
            if visited > MAX_VISITED_DIRS || repos.len() >= MAX_SCAN_RESULTS {
                return repos;
            }
            let p = parent.join(&name);
            if is_git_repo(&p) {
                repos.push(scanned_repo(&p));
            }
        }
    }
    repos
}

fn scanned_repo(dir: &Path) -> ScannedRepo {
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let res = detect(dir);
    ScannedRepo {
        name,
        root_dir: dir.to_string_lossy().to_string(),
        ecosystems: res.ecosystems,
        suggestions: res.suggestions,
    }
}

fn unique_ecosystems(suggestions: &[Suggestion]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in suggestions {
        if !out.contains(&s.ecosystem) {
            out.push(s.ecosystem.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn node_scripts_picked_by_priority_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "package.json", r#"{"scripts":{"start":"x","dev":"y","build":"z","serve":"w"}}"#);
        let res = detect(dir.path());
        let cmds: Vec<&str> = res.suggestions.iter().map(|s| s.command.as_str()).collect();
        assert_eq!(cmds, vec!["npm run dev", "npm run start"]);
        assert_eq!(res.ecosystems, vec!["node"]);
        assert_eq!(res.suggestions[0].name, "dev");
    }

    #[test]
    fn node_package_manager_from_lockfiles() {
        for (lock, pm) in [
            ("pnpm-lock.yaml", "pnpm"),
            ("yarn.lock", "yarn"),
            ("bun.lockb", "bun"),
            ("bun.lock", "bun"),
            ("package-lock.json", "npm"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            write_file(dir.path(), "package.json", r#"{"scripts":{"dev":"vite"}}"#);
            write_file(dir.path(), lock, "");
            let res = detect(dir.path());
            assert_eq!(res.suggestions[0].command, format!("{pm} run dev"), "{lock}");
        }
    }

    #[test]
    fn node_defaults_to_npm_without_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "package.json", r#"{"scripts":{"dev":"vite"}}"#);
        assert_eq!(detect(dir.path()).suggestions[0].command, "npm run dev");
    }

    #[test]
    fn node_bad_json_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "package.json", "{ not json");
        assert!(detect(dir.path()).suggestions.is_empty());
    }

    #[test]
    fn subdirs_make_items_with_workdir_and_names() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "frontend/package.json", r#"{"scripts":{"dev":"vite"}}"#);
        write_file(dir.path(), "backend/package.json", r#"{"scripts":{"start":"node ."}}"#);
        let res = detect(dir.path());
        assert_eq!(res.suggestions.len(), 2);
        assert_eq!(res.suggestions[0].name, "backend");
        assert_eq!(res.suggestions[0].work_dir.as_deref(), Some("backend"));
        assert_eq!(res.suggestions[0].command, "npm run start");
        assert_eq!(res.suggestions[1].name, "frontend");
    }

    #[test]
    fn subdir_second_script_gets_dir_prefix() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "web/package.json", r#"{"scripts":{"dev":"vite","start":"serve"}}"#);
        let res = detect(dir.path());
        assert_eq!(res.suggestions[0].name, "web");
        assert_eq!(res.suggestions[1].name, "web: start");
    }

    #[test]
    fn skip_dirs_are_not_scanned() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "node_modules/pkg/package.json", r#"{"scripts":{"dev":"x"}}"#);
        write_file(dir.path(), ".hidden/package.json", r#"{"scripts":{"dev":"x"}}"#);
        assert!(detect(dir.path()).suggestions.is_empty());
    }

    #[test]
    fn empty_dir_yields_empty_result() {
        let dir = tempfile::tempdir().unwrap();
        let res = detect(dir.path());
        assert!(res.suggestions.is_empty());
        assert!(res.ecosystems.is_empty());
    }

    #[test]
    fn total_suggestions_capped_at_eight() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "package.json", r#"{"scripts":{"dev":"x","start":"x"}}"#);
        for name in ["a", "b", "c", "d", "e"] {
            write_file(dir.path(), &format!("{name}/package.json"), r#"{"scripts":{"dev":"x","start":"x"}}"#);
        }
        assert_eq!(detect(dir.path()).suggestions.len(), 8);
    }

    #[test]
    fn rust_package_suggests_cargo_run() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "Cargo.toml", "[package]\nname = \"app\"\n");
        let res = detect(dir.path());
        assert_eq!(res.suggestions[0].command, "cargo run");
        assert_eq!(res.suggestions[0].name, "run");
        assert_eq!(res.ecosystems, vec!["rust"]);
    }

    #[test]
    fn rust_workspace_only_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "Cargo.toml", "[workspace]\nmembers = [\"a\"]\n");
        assert!(detect(dir.path()).suggestions.is_empty());
    }

    #[test]
    fn go_root_main_and_cmd_entry() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "go.mod", "module app\n");
        write_file(dir.path(), "main.go", "package main\n");
        assert_eq!(detect(dir.path()).suggestions[0].command, "go run .");

        let dir2 = tempfile::tempdir().unwrap();
        write_file(dir2.path(), "go.mod", "module app\n");
        write_file(dir2.path(), "cmd/api/main.go", "package main\n");
        assert_eq!(detect(dir2.path()).suggestions[0].command, "go run ./cmd/api");
    }

    #[test]
    fn go_without_entry_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "go.mod", "module app\n");
        assert!(detect(dir.path()).suggestions.is_empty());
    }

    #[test]
    fn python_entry_priority() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "app.py", "");
        write_file(dir.path(), "main.py", "");
        write_file(dir.path(), "manage.py", "");
        let res = detect(dir.path());
        assert_eq!(res.suggestions[0].command, "python manage.py runserver");
        assert_eq!(res.suggestions[0].name, "manage");
    }

    #[test]
    fn python_without_entry_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "requirements.txt", "fastapi\n");
        write_file(dir.path(), "pyproject.toml", "[project]\nname = \"app\"\n");
        assert!(detect(dir.path()).suggestions.is_empty());
    }

    #[test]
    fn is_git_repo_accepts_dir_or_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        fs::create_dir_all(a.join(".git")).unwrap();
        assert!(is_git_repo(&a));
        let b = dir.path().join("b");
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join(".git"), "gitdir: ../x").unwrap();
        assert!(is_git_repo(&b));
        assert!(!is_git_repo(dir.path()));
    }

    #[test]
    fn scan_finds_repos_at_depth_one_and_two() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("repoA/.git")).unwrap();
        fs::create_dir_all(dir.path().join("org/repoB/.git")).unwrap();
        fs::create_dir_all(dir.path().join("org/notrepo")).unwrap();
        let repos = scan(dir.path());
        let names: Vec<&str> = repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["repoB", "repoA"]);
    }

    #[test]
    fn scan_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".hidden/repo/.git")).unwrap();
        assert!(scan(dir.path()).is_empty());
    }

    #[test]
    fn scan_root_is_repo_returns_self() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let repos = scan(dir.path());
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].root_dir, dir.path().to_string_lossy());
    }

    #[test]
    fn scan_caps_results_at_200() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..201 {
            fs::create_dir_all(dir.path().join(format!("repo{i:03}")).join(".git")).unwrap();
        }
        assert_eq!(scan(dir.path()).len(), MAX_SCAN_RESULTS);
    }

    #[test]
    fn scan_repos_include_detected_suggestions() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("web/.git")).unwrap();
        write_file(dir.path(), "web/package.json", r#"{"scripts":{"dev":"vite"}}"#);
        let repos = scan(dir.path());
        assert_eq!(repos[0].suggestions[0].command, "npm run dev");
        assert_eq!(repos[0].ecosystems, vec!["node"]);
    }
}

use std::ffi::OsStr;
use std::path::PathBuf;

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
}

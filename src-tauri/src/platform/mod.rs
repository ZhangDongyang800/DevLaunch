#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{join_lines, spawn_script};

#[cfg(windows)]
pub use windows::{
    build_wt_commandline, cmd_launch_args, cmd_pane_command, encode_ps_command, fold_cmd_lines,
    ps_launch_args, ps_pane_script, resolve_wt_path, resolve_wt_with, spawn_panes, LaunchMode,
    PaneSpec,
};

#[cfg(not(windows))]
pub struct PaneSpec {
    pub title: String,
    pub work_dir: std::path::PathBuf,
    pub shell: crate::config::Shell,
    pub command: String,
}

#[cfg(not(windows))]
pub enum LaunchMode {
    WindowsTerminal,
    Fallback,
}

#[cfg(not(windows))]
pub fn spawn_panes(_project_name: &str, _panes: &[PaneSpec]) -> std::io::Result<LaunchMode> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "terminal launching is only implemented on Windows",
    ))
}

#[cfg(not(windows))]
pub fn join_lines(command: &str, sep: &str) -> String {
    command
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<&str>>()
        .join(sep)
}

#[cfg(not(windows))]
pub fn spawn_script(
    _terminal: crate::config::Terminal,
    _work_dir: &str,
    _script_path: &str,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "terminal launching is only implemented on Windows in v1",
    ))
}

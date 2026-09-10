#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{
    build_wt_commandline, cmd_launch_args, cmd_pane_command, encode_ps_command, fold_cmd_lines,
    ps_launch_args, ps_pane_script, resolve_wt_path, resolve_wt_with, spawn_panes,
    validate_wt_commandline, LaunchMode, PaneSpec,
};

#[cfg(not(windows))]
#[derive(Debug)]
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

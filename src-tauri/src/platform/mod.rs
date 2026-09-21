#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{
    bash_launch_args, bash_pane_script, build_wt_commandline, cmd_launch_args, cmd_pane_command,
    encode_ps_command, plan_spawn, ps_launch_args, ps_pane_script, resolve_bash_path,
    resolve_bash_with, resolve_wt_path, resolve_wt_with, spawn_panes, to_msys_path,
    validate_wt_commandline, FallbackLaunch, LaunchMode, PaneSpec, SpawnPlan,
};

#[cfg(not(windows))]
#[derive(Debug)]
pub struct PaneSpec {
    pub title: String,
    pub work_dir: std::path::PathBuf,
    pub shell: crate::config::Shell,
    pub command: String,
    pub env: Vec<(String, String)>,
}

// 非 Windows 目标：本产品只在 Windows 上运行，但 stub 也要能编译通过，
// 否则 `cargo test` / `cargo check` 在别的平台上连纯函数测试都跑不了。
// `PartialEq` 是必须的——`launcher.rs` 用 `mode == LaunchMode::Fallback` 比较。
#[cfg(not(windows))]
#[derive(Debug, PartialEq, Eq)]
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

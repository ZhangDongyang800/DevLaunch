#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{join_lines, spawn_script};

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

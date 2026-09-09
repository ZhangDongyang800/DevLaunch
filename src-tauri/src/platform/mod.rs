#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::spawn;

#[cfg(not(windows))]
pub fn spawn(
    _terminal: crate::config::Terminal,
    _work_dir: &str,
    _command: &str,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "terminal launching is only implemented on Windows in v1",
    ))
}

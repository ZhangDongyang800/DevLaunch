//! 极简文件日志。
//!
//! release 是 `windows_subsystem = "windows"`：**没有控制台**，所有 `eprintln!`
//! 直接丢失。而 `[profile.release]` 又是 `panic = "abort"` + `strip = true`，
//! panic 连栈都没有。结果是任何失败都留不下现场——用户只能说"点了没反应"，
//! 作者拿不到任何可诊断的输入。
//!
//! 这里不引入 `log` / `tracing`：只有一个 sink、一个级别，几十行足够，
//! 也符合本项目"依赖面极小"的取舍。日志写在
//! `%APPDATA%\com.devlaunch.app\logs\devlaunch.log`，单文件有界。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 单文件上限；超过就滚动成 `.1`（下一次运行启动时执行滚动）。
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

static LOG_FILE: OnceLock<Option<PathBuf>> = OnceLock::new();
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 初始化日志目录。重复调用无效果；目录建不出来时后续写入全部静默丢弃。
pub fn init(dir: &Path) {
    let _ = LOG_FILE.set(prepare(dir));
}

fn prepare(dir: &Path) -> Option<PathBuf> {
    fs::create_dir_all(dir).ok()?;
    let file = dir.join("devlaunch.log");
    // 上次运行涨过上限的日志：滚动一份再重新开始，避免无限增长。
    let oversized = fs::metadata(&file).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false);
    if oversized {
        let _ = fs::remove_file(dir.join("devlaunch.log.1"));
        let _ = fs::rename(&file, dir.join("devlaunch.log.1"));
    }
    Some(file)
}

/// 日志所在目录；未初始化时返回 `None`（此时写入全部丢弃）。
pub fn log_dir() -> Option<PathBuf> {
    LOG_FILE
        .get()
        .and_then(|o| o.as_ref())
        .and_then(|f| f.parent().map(Path::to_path_buf))
}

/// 日志文件的完整路径；供「打开日志目录」入口与排障使用。
pub fn log_file() -> Option<PathBuf> {
    LOG_FILE.get().and_then(|o| o.clone())
}

pub fn error(msg: impl AsRef<str>) {
    write("ERROR", msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    write("WARN", msg.as_ref());
}

pub fn info(msg: impl AsRef<str>) {
    write("INFO", msg.as_ref());
}

fn write(level: &str, msg: &str) {
    // debug 构建同时打到 stderr，`tauri dev` 里能直接看到，不必去翻文件。
    #[cfg(debug_assertions)]
    eprintln!("[{level}] {msg}");

    let Some(path) = LOG_FILE.get().and_then(|o| o.as_ref()) else { return };
    // 锁被污染时宁可丢一条日志，也不要让"写日志"变成新的崩溃点。
    let Ok(_guard) = WRITE_LOCK.lock() else { return };
    // 换行会破坏"一行一条"的可读性，压成空格。
    let line = format!("{} [{level}] {}\n", timestamp(), msg.replace(['\n', '\r'], " "));
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) else { return };
    if f.metadata().map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
        // 运行期间涨过上限：先截断保证有界，下一次启动会滚动出 `.1`。
        let _ = f.set_len(0);
    }
    let _ = f.write_all(line.as_bytes());
}

fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// Unix 秒 → UTC 民用日期时间（Howard Hinnant 的 `civil_from_days`）。
/// 自己算而不引 `chrono`：只需要 UTC 的一个固定格式，没有时区/夏令时需求。
fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_unix_matches_known_instants() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        // 2026-09-21 00:00:00 UTC（= 第 20717 天）
        assert_eq!(civil_from_unix(1_789_948_800), (2026, 9, 21, 0, 0, 0));
        // 闰年 2 月 29 日
        assert_eq!(civil_from_unix(1_709_164_800), (2024, 2, 29, 0, 0, 0));
        // 年末最后一秒
        assert_eq!(civil_from_unix(1_735_689_599), (2024, 12, 31, 23, 59, 59));
        // 世纪闰年规则：2000 年是闰年（能被 400 整除）
        assert_eq!(civil_from_unix(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn write_is_a_noop_before_init_and_never_panics() {
        // 未初始化时写入必须静默丢弃，而不是 panic（日志不能变成新的崩溃点）。
        error("before init");
        assert!(log_dir().is_none() || log_file().is_some());
    }

    #[test]
    fn init_creates_file_and_flattens_newlines() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        // 直接调 prepare 而不是 init：OnceLock 在一个进程里只能设一次，
        // 用 init 会让本测试与其他测试互相污染。
        let file = prepare(&logs).expect("log file");
        assert!(logs.is_dir());
        let mut f = OpenOptions::new().create(true).append(true).open(&file).unwrap();
        let line = format!("{} [ERROR] {}\n", timestamp(), "a\nb".replace(['\n', '\r'], " "));
        f.write_all(line.as_bytes()).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("[ERROR] a b"), "{text}");
        assert_eq!(text.lines().count(), 1, "{text}");
    }

    #[test]
    fn prepare_rolls_an_oversized_log() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("devlaunch.log");
        fs::write(&file, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).unwrap();
        let got = prepare(dir.path()).unwrap();
        assert_eq!(got, file);
        assert!(dir.path().join("devlaunch.log.1").is_file());
        assert!(!file.exists(), "oversized log should have been moved aside");
    }
}

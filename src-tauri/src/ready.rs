use crate::config::ReadyCondition;
use std::net::TcpStream;
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug)]
pub struct ReadyTimeout {
    pub description: String,
}

pub fn wait_ready(cond: &ReadyCondition, default_timeout_sec: u64) -> Result<(), ReadyTimeout> {
    match cond {
        ReadyCondition::Immediate => Ok(()),
        ReadyCondition::Delay { seconds } => {
            sleep(Duration::from_secs(*seconds));
            Ok(())
        }
        ReadyCondition::Port { port, host, timeout_sec } => {
            let addr = format!("{host}:{port}");
            poll(
                format!("端口 {addr} 不可连接"),
                *timeout_sec,
                default_timeout_sec,
                Duration::from_millis(500),
                || TcpStream::connect(&addr).is_ok(),
            )
        }
        ReadyCondition::Process { process_name, timeout_sec } => {
            let name = process_name.clone();
            poll(
                format!("进程 {process_name} 未运行"),
                *timeout_sec,
                default_timeout_sec,
                Duration::from_secs(1),
                || process_alive(&name),
            )
        }
    }
}

fn poll<F: Fn() -> bool>(
    timeout_desc: String,
    timeout_sec: u64,
    default_timeout_sec: u64,
    interval: Duration,
    check: F,
) -> Result<(), ReadyTimeout> {
    let effective = if timeout_sec == 0 { default_timeout_sec } else { timeout_sec };
    let deadline = std::time::Instant::now() + Duration::from_secs(effective);
    loop {
        if check() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(ReadyTimeout { description: timeout_desc });
        }
        sleep(interval);
    }
}

fn process_alive(wanted: &str) -> bool {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| process_matches(&p.name().to_string_lossy(), wanted))
}

pub fn process_matches(actual: &str, wanted: &str) -> bool {
    fn strip_exe(s: &str) -> &str {
        s.strip_suffix(".exe").unwrap_or(s)
    }
    strip_exe(actual).eq_ignore_ascii_case(strip_exe(wanted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReadyCondition;
    use std::net::TcpListener;
    use std::time::Instant;

    #[test]
    fn immediate_is_ready() {
        assert!(wait_ready(&ReadyCondition::Immediate, 30).is_ok());
    }

    #[test]
    fn delay_waits_at_least_given_seconds() {
        let start = Instant::now();
        wait_ready(&ReadyCondition::Delay { seconds: 1 }, 30).unwrap();
        assert!(start.elapsed() >= std::time::Duration::from_millis(1000));
    }

    #[test]
    fn port_ready_when_listener_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cond = ReadyCondition::Port {
            port,
            host: "127.0.0.1".into(),
            timeout_sec: 5,
        };
        wait_ready(&cond, 30).unwrap();
    }

    #[test]
    fn port_timeout_when_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // 端口已释放，连接应失败
        let cond = ReadyCondition::Port {
            port,
            host: "127.0.0.1".into(),
            timeout_sec: 1,
        };
        assert!(wait_ready(&cond, 30).is_err());
    }

    #[test]
    fn zero_timeout_falls_back_to_default() {
        // 用一个必然关闭的端口 + 0 超时：若 0 被当作"立即超时"则瞬间失败；
        // 回退 default_timeout_sec=1 时耗时至少约 1 秒。
        let cond = ReadyCondition::Port {
            port: 1,
            host: "127.0.0.1".into(),
            timeout_sec: 0,
        };
        let start = Instant::now();
        assert!(wait_ready(&cond, 1).is_err());
        assert!(start.elapsed() >= std::time::Duration::from_millis(900));
    }

    #[cfg(windows)]
    #[test]
    fn process_found_when_running() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // 占位避免 unused import 警告
        let cond = ReadyCondition::Process {
            process_name: "explorer.exe".into(),
            timeout_sec: 5,
        };
        wait_ready(&cond, 30).unwrap();
        drop(listener);
    }

    #[test]
    fn process_matches_ignores_case_and_exe_suffix() {
        assert!(process_matches("explorer.exe", "explorer.exe"));
        assert!(process_matches("explorer.exe", "EXPLORER"));
        assert!(process_matches("python", "python.exe"));
        assert!(!process_matches("explorer.exe", "python.exe"));
    }

    #[test]
    fn process_timeout_when_not_running() {
        let cond = ReadyCondition::Process {
            process_name: "definitely-not-running-xyz123.exe".into(),
            timeout_sec: 1,
        };
        assert!(wait_ready(&cond, 30).is_err());
    }
}

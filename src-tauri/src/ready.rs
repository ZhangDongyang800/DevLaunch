//! 就绪条件的「终端内」表达：v2 起一组共享一个终端窗口，步骤间的就绪等待
//! 不再由 Rust 侧同步轮询，而是转译为脚本片段写进临时启动脚本。
//!
//! - cmd 脚本：统一借道 `powershell -NoProfile -Command` one-liner（cmd/wt 通用），
//!   超时后 echo 提示 + pause 并以非零码退出，停止组内后续步骤。
//! - powershell 脚本：原生代码块。
use crate::config::ReadyCondition;

pub fn effective_timeout(cond: &ReadyCondition, default_timeout_sec: u64) -> u64 {
    match cond {
        ReadyCondition::Port { timeout_sec, .. } | ReadyCondition::Process { timeout_sec, .. }
            if *timeout_sec > 0 =>
        {
            *timeout_sec
        }
        _ => default_timeout_sec,
    }
}

/// 进程名去掉 .exe 后缀（大小写不敏感），供 Get-Process -Name 使用。
pub fn ps_process_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let stripped = lower.strip_suffix(".exe").unwrap_or(&lower);
    stripped.to_string()
}

/// echo/Write-Host 用的安全标签：去掉会破坏 batch/PS 语法的字符。
fn safe_label(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '&' | '|' | '<' | '>' | '"' | '^' | '%' | '!' | '(' | ')' | '\'' | '`'))
        .collect()
}

fn poll_expr_port(host: &str, port: u16) -> String {
    format!(
        "Test-NetConnection '{}' -Port {} -InformationLevel Quiet -WarningAction SilentlyContinue",
        host.replace('\'', "''"),
        port
    )
}

fn poll_expr_process(name: &str) -> String {
    format!("Get-Process -Name '{}' -ErrorAction SilentlyContinue", ps_process_name(name))
}

/// powershell 轮询 one-liner：t 为半秒次数，就绪 exit 0，超时 exit 1。cmd 与 ps 脚本通用。
fn poll_one_liner(expr: &str, timeout_sec: u64) -> String {
    let tries = timeout_sec * 2;
    format!(
        "powershell -NoProfile -Command \"$tries={tries}; while($tries -gt 0){{ if({expr}){{ exit 0 }}; Start-Sleep -Milliseconds 500; $tries=$tries-1 }} exit 1\""
    )
}

/// cmd（含 wt）脚本的等待块；Immediate 返回空串。
pub fn wait_block_cmd(cond: &ReadyCondition, default_timeout_sec: u64, step_name: &str) -> String {
    let label = safe_label(step_name);
    match cond {
        ReadyCondition::Immediate => String::new(),
        ReadyCondition::Delay { seconds } if *seconds == 0 => String::new(),
        ReadyCondition::Delay { seconds } => format!("timeout /t {seconds} /nobreak >nul"),
        ReadyCondition::Port { port, host, timeout_sec: _ } => {
            let t = effective_timeout(cond, default_timeout_sec);
            format!(
                "{}\r\nif errorlevel 1 echo [DevLaunch] step \"{}\" NOT READY - port {}:{} - timeout {}s && pause && exit /b 1",
                poll_one_liner(&poll_expr_port(host, *port), t),
                label,
                host,
                port,
                t
            )
        }
        ReadyCondition::Process { process_name, timeout_sec: _ } => {
            let t = effective_timeout(cond, default_timeout_sec);
            format!(
                "{}\r\nif errorlevel 1 echo [DevLaunch] step \"{}\" NOT READY - process {} - timeout {}s && pause && exit /b 1",
                poll_one_liner(&poll_expr_process(process_name), t),
                label,
                ps_process_name(process_name),
                t
            )
        }
    }
}

/// powershell 脚本的等待块；Immediate 返回空串。
pub fn wait_block_ps(cond: &ReadyCondition, default_timeout_sec: u64, step_name: &str) -> String {
    let label = safe_label(step_name);
    match cond {
        ReadyCondition::Immediate => String::new(),
        ReadyCondition::Delay { seconds } if *seconds == 0 => String::new(),
        ReadyCondition::Delay { seconds } => format!("Start-Sleep -Seconds {seconds}"),
        ReadyCondition::Port { port, host, timeout_sec: _ } => {
            let t = effective_timeout(cond, default_timeout_sec);
            let tries = t * 2;
            let expr = poll_expr_port(host, *port);
            format!(
                "$tries = {tries}\r\nwhile($tries -gt 0){{ if({expr}){{ break }}; Start-Sleep -Milliseconds 500; $tries = $tries - 1 }}\r\nif($tries -le 0){{ Write-Host '[DevLaunch] step {label} NOT READY - port {host}:{port} - timeout {t}s'; Read-Host 'Press Enter to exit'; exit 1 }}"
            )
        }
        ReadyCondition::Process { process_name, timeout_sec: _ } => {
            let t = effective_timeout(cond, default_timeout_sec);
            let tries = t * 2;
            let expr = poll_expr_process(process_name);
            let name = ps_process_name(process_name);
            format!(
                "$tries = {tries}\r\nwhile($tries -gt 0){{ if({expr}){{ break }}; Start-Sleep -Milliseconds 500; $tries = $tries - 1 }}\r\nif($tries -le 0){{ Write-Host '[DevLaunch] step {label} NOT READY - process {name} - timeout {t}s'; Read-Host 'Press Enter to exit'; exit 1 }}"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port_cond(timeout_sec: u64) -> ReadyCondition {
        ReadyCondition::Port { port: 8081, host: "127.0.0.1".into(), timeout_sec }
    }

    #[test]
    fn immediate_yields_empty_block() {
        assert_eq!(wait_block_cmd(&ReadyCondition::Immediate, 30, "x"), "");
        assert_eq!(wait_block_ps(&ReadyCondition::Immediate, 30, "x"), "");
    }

    #[test]
    fn zero_delay_yields_empty_block() {
        let d = ReadyCondition::Delay { seconds: 0 };
        assert_eq!(wait_block_cmd(&d, 30, "x"), "");
        assert_eq!(wait_block_ps(&d, 30, "x"), "");
    }

    #[test]
    fn delay_translated_per_dialect() {
        let d = ReadyCondition::Delay { seconds: 5 };
        assert_eq!(wait_block_cmd(&d, 30, "x"), "timeout /t 5 /nobreak >nul");
        assert_eq!(wait_block_ps(&d, 30, "x"), "Start-Sleep -Seconds 5");
    }

    #[test]
    fn port_cmd_block_polls_then_guards() {
        let s = wait_block_cmd(&port_cond(30), 30, "server");
        assert!(s.contains("$tries=60"));
        assert!(s.contains("Test-NetConnection '127.0.0.1' -Port 8081 -InformationLevel Quiet"));
        assert!(s.contains("exit 0"));
        assert!(s.contains("if errorlevel 1"));
        assert!(s.contains(r#"step "server" NOT READY"#));
        assert!(s.contains("pause && exit /b 1"));
    }

    #[test]
    fn port_zero_timeout_falls_back_to_default() {
        let s = wait_block_cmd(&port_cond(0), 15, "x");
        assert!(s.contains("$tries=30"));
    }

    #[test]
    fn port_ps_block_polls_then_guards() {
        let s = wait_block_ps(&port_cond(10), 30, "api");
        assert!(s.contains("$tries = 20"));
        assert!(s.contains("break"));
        assert!(s.contains("Write-Host '[DevLaunch] step api NOT READY"));
        assert!(s.contains("exit 1"));
    }

    #[test]
    fn process_block_strips_exe_suffix() {
        let cond = ReadyCondition::Process {
            process_name: "python.exe".into(),
            timeout_sec: 5,
        };
        let s = wait_block_cmd(&cond, 30, "x");
        assert!(s.contains("Get-Process -Name 'python' -ErrorAction SilentlyContinue"));
        assert!(s.contains("timeout 5s"));
        let ps = wait_block_ps(&cond, 30, "x");
        assert!(ps.contains("Get-Process -Name 'python'"));
    }

    #[test]
    fn ps_process_name_case_insensitive() {
        assert_eq!(ps_process_name("Python.EXE"), "python");
        assert_eq!(ps_process_name("node"), "node");
    }

    #[test]
    fn label_strips_dangerous_chars() {
        let cond = ReadyCondition::Process { process_name: "a.exe".into(), timeout_sec: 1 };
        let s = wait_block_cmd(&cond, 30, "a & b | c");
        assert!(s.contains(r#"step "a  b  c" NOT READY"#));
        let ps = wait_block_ps(&cond, 30, "it's");
        assert!(ps.contains("step its NOT READY"));
    }
}

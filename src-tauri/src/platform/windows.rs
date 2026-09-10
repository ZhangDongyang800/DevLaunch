use crate::config::Terminal;
use std::process::Command;

/// 多行命令合并为单行（同一终端窗口内顺序执行）：去空行、trim，按 shell 语义连接。
pub fn join_lines(command: &str, sep: &str) -> String {
    command
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<&str>>()
        .join(sep)
}

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn cmd_script_args(script_path: &str) -> String {
    format!("/K call \"{script_path}\"")
}

pub fn ps_script_args(script_path: &str) -> String {
    format!("-NoExit -ExecutionPolicy Bypass -File \"{script_path}\"")
}

pub fn wt_script_args(work_dir: &str, script_path: &str) -> String {
    format!("-d \"{work_dir}\" cmd /K call \"{script_path}\"")
}

/// 启动一个终端窗口执行临时脚本文件（组内所有步骤 + 就绪等待都在脚本里）。
/// cmd/wt 走 `cmd /K call`，powershell 走 `-File`；窗口常驻（-NoExit / /K）。
pub fn spawn_script(terminal: Terminal, work_dir: &str, script_path: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    let args = match terminal {
        Terminal::Cmd => cmd_script_args(script_path),
        Terminal::PowerShell => ps_script_args(script_path),
        Terminal::WindowsTerminal => {
            // wt 不加 CREATE_NEW_CONSOLE：由 wt 自身管理窗口
            let mut c = Command::new("wt");
            c.current_dir(work_dir).raw_arg(wt_script_args(work_dir, script_path));
            return c.spawn().map(|_| ());
        }
    };
    let program = match terminal {
        Terminal::PowerShell => "powershell",
        _ => "cmd",
    };
    let mut c = Command::new(program);
    c.current_dir(work_dir).raw_arg(args);
    c.creation_flags(CREATE_NEW_CONSOLE);
    c.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_lines_basic() {
        assert_eq!(join_lines("a\n b \n\nc", " && "), "a && b && c");
        assert_eq!(join_lines("a\nb", "; "), "a; b");
        assert_eq!(join_lines("\n \n", " && "), "");
    }

    #[test]
    fn script_args_per_terminal() {
        assert_eq!(cmd_script_args(r"C:\Temp\a.cmd"), r#"/K call "C:\Temp\a.cmd""#);
        assert_eq!(
            ps_script_args(r"C:\Temp\a.ps1"),
            r#"-NoExit -ExecutionPolicy Bypass -File "C:\Temp\a.ps1""#
        );
        assert_eq!(
            wt_script_args(r"D:\proj", r"C:\Temp\a.cmd"),
            r#"-d "D:\proj" cmd /K call "C:\Temp\a.cmd""#
        );
    }
}

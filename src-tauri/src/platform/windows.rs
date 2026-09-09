use crate::config::Terminal;
use std::process::Command;

/// 多行命令合并为单行（同一终端窗口内顺序执行）：去空行、trim，按 shell 语义连接。
fn join_lines(command: &str, sep: &str) -> String {
    command
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<&str>>()
        .join(sep)
}

pub fn cmd_command_line(work_dir: &str, command: &str) -> String {
    format!("/K cd /d \"{work_dir}\" && {}", join_lines(command, " && "))
}

pub fn powershell_command_line(work_dir: &str, command: &str) -> String {
    format!(
        "-NoExit -Command \"Set-Location '{work_dir}'; {}\"",
        join_lines(command, "; ")
    )
}

pub fn wt_command_line(work_dir: &str, command: &str) -> String {
    format!("-d \"{work_dir}\" cmd /K {}", join_lines(command, " && "))
}

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn spawn(terminal: Terminal, work_dir: &str, command: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    match terminal {
        Terminal::Cmd => {
            let mut c = Command::new("cmd");
            c.raw_arg(cmd_command_line(work_dir, command));
            c.creation_flags(CREATE_NEW_CONSOLE);
            c.spawn().map(|_| ())
        }
        Terminal::PowerShell => {
            let mut c = Command::new("powershell");
            c.raw_arg(powershell_command_line(work_dir, command));
            c.creation_flags(CREATE_NEW_CONSOLE);
            c.spawn().map(|_| ())
        }
        Terminal::WindowsTerminal => {
            let mut c = Command::new("wt");
            c.raw_arg(wt_command_line(work_dir, command));
            c.spawn().map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_line_format() {
        let s = cmd_command_line(r"D:\my dir", "npm run dev");
        assert_eq!(s, r#"/K cd /d "D:\my dir" && npm run dev"#);
    }

    #[test]
    fn powershell_line_format() {
        let s = powershell_command_line(r"D:\my dir", "python app.py");
        assert_eq!(s, "-NoExit -Command \"Set-Location 'D:\\my dir'; python app.py\"");
    }

    #[test]
    fn wt_line_format() {
        let s = wt_command_line(r"D:\my dir", "npm run dev");
        assert_eq!(s, r#"-d "D:\my dir" cmd /K npm run dev"#);
    }

    #[test]
    fn multiline_command_joined_for_cmd() {
        let s = cmd_command_line(r"D:\proj", "conda activate xingtu\npython -m uvicorn main:app");
        assert_eq!(
            s,
            r#"/K cd /d "D:\proj" && conda activate xingtu && python -m uvicorn main:app"#
        );
    }

    #[test]
    fn multiline_command_joined_for_powershell() {
        let s = powershell_command_line(r"D:\proj", "conda activate x\npython run.py");
        assert_eq!(
            s,
            "-NoExit -Command \"Set-Location 'D:\\proj'; conda activate x; python run.py\""
        );
    }

    #[test]
    fn multiline_blank_and_whitespace_lines_dropped() {
        let s = cmd_command_line("D:\\p", "\n  a  \r\n\n \n b \n");
        assert_eq!(s, r#"/K cd /d "D:\p" && a && b"#);
    }
}

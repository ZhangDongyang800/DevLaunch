use crate::config::Terminal;
use std::process::Command;

pub fn cmd_command_line(work_dir: &str, command: &str) -> String {
    format!("/K cd /d \"{work_dir}\" && {command}")
}

pub fn powershell_command_line(work_dir: &str, command: &str) -> String {
    format!("-NoExit -Command \"Set-Location '{work_dir}'; {command}\"")
}

pub fn wt_command_line(work_dir: &str, command: &str) -> String {
    format!("-d \"{work_dir}\" cmd /K {command}")
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
}

use crate::config::Shell;
use std::ffi::OsStr;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const MAX_COMMANDLINE_UTF16: usize = 30_000;

#[derive(Debug)]
pub struct PaneSpec {
    pub title: String,
    pub work_dir: PathBuf,
    pub shell: Shell,
    pub command: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchMode {
    WindowsTerminal,
    Fallback,
}

#[derive(Debug)]
pub struct FallbackLaunch {
    pub program: String,
    pub args: String,
    pub work_dir: PathBuf,
}

#[derive(Debug)]
pub enum SpawnPlan {
    Wt { program: PathBuf, args: String },
    Fallback { launches: Vec<FallbackLaunch> },
}

/// 回显文本转义：cmd 元字符加 ^，避免 echo 参数被解析为连接/重定向。
fn escape_echo_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        if matches!(ch, '^' | '&' | '|' | '<' | '>') {
            out.push('^');
        }
        out.push(ch);
    }
    out
}

/// 先 cd 到工作目录，再逐行回显「目录>命令」后执行，视觉上等同手动输入。
pub fn cmd_pane_command(work_dir: &Path, command: &str) -> String {
    let dir = work_dir.display().to_string();
    let cd = format!("cd /d \"{dir}\"");
    let lines: Vec<&str> = command.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return cd;
    }
    let mut parts = vec![cd];
    for line in lines {
        parts.push(format!("echo {}", escape_echo_text(&format!("{dir}>{line}"))));
        parts.push(line.to_string());
    }
    parts.join(" && ")
}

pub fn ps_pane_script(work_dir: &Path, command: &str) -> String {
    let cd = format!(
        "Set-Location -LiteralPath '{}'",
        work_dir.display().to_string().replace('\'', "''")
    );
    if command.trim().is_empty() { cd } else { format!("{cd}\r\n{command}") }
}

pub fn encode_ps_command(script: &str) -> String {
    use base64::Engine;
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// wt 用 CommandLineToArgvW 解析参数：结尾反斜杠会逃逸收尾引号，必须加倍。
fn quote_wt_arg(value: &str) -> String {
    let mut out = value.replace('"', "'");
    let trailing = out.chars().rev().take_while(|c| *c == '\\').count();
    if trailing > 0 {
        out.push_str(&"\\".repeat(trailing));
    }
    format!("\"{out}\"")
}

pub fn resolve_wt_with(
    override_path: Option<&OsStr>,
    path_env: Option<&OsStr>,
    local_appdata: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if p.is_empty() {
            return None;
        }
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let cand = dir.join("wt.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let local = local_appdata?;
    let cand = PathBuf::from(local).join("Microsoft").join("WindowsApps").join("wt.exe");
    cand.is_file().then_some(cand)
}

pub fn resolve_wt_path() -> Option<PathBuf> {
    resolve_wt_with(
        std::env::var_os("DEVLAUNCH_WT_PATH").as_deref(),
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("LOCALAPPDATA").as_deref(),
    )
}

/// Windows 路径 → MSYS/Git Bash 路径：D:\a\b → /d/a/b；UNC → //server/share/...。
pub fn to_msys_path(path: &Path) -> String {
    let s = path.display().to_string();
    if let Some(rest) = s.strip_prefix("\\\\") {
        return format!("//{}", rest.replace('\\', "/"));
    }
    let normalized = s.replace('\\', "/");
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = normalized[2..].trim_start_matches('/');
        return if rest.is_empty() { format!("/{drive}") } else { format!("/{drive}/{rest}") };
    }
    normalized
}

/// bash 窗格脚本：cd → 用户命令原文（不逐行回显）→ exec 保持窗口。
pub fn bash_pane_script(work_dir: &Path, command: &str) -> String {
    let command = command.replace("\r\n", "\n");
    let dir = to_msys_path(work_dir).replace('\'', "'\\''");
    let mut script = format!("cd '{dir}'");
    if !command.trim().is_empty() {
        script.push('\n');
        script.push_str(&command);
    }
    script.push_str("\nexec bash -il");
    script
}

pub fn encode_bash_script(script: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(script.as_bytes())
}

/// 固定外壳：用户内容只存在于 base64，不参与任何引号解析。
pub fn bash_launch_args(work_dir: &Path, command: &str) -> String {
    let b64 = encode_bash_script(&bash_pane_script(work_dir, command));
    format!("-lc \"bash -l <(base64 -d<<<{b64})\"")
}

pub fn resolve_bash_with(
    override_path: Option<&OsStr>,
    program_files: Option<&OsStr>,
    program_files_x86: Option<&OsStr>,
    path_env: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if p.is_empty() {
            return None;
        }
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    for root in [program_files, program_files_x86].into_iter().flatten() {
        let cand = PathBuf::from(root).join("Git").join("bin").join("bash.exe");
        if cand.is_file() {
            return Some(cand);
        }
    }
    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let lower = dir.to_string_lossy().to_ascii_lowercase();
            let lower = lower.trim_end_matches(['\\', '/']);
            if lower.ends_with("system32") || lower.ends_with("windowsapps") {
                continue;
            }
            let cand = dir.join("bash.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

pub fn resolve_bash_path() -> Option<PathBuf> {
    resolve_bash_with(
        std::env::var_os("DEVLAUNCH_BASH_PATH").as_deref(),
        std::env::var_os("ProgramFiles").as_deref(),
        std::env::var_os("ProgramFiles(x86)").as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
}

pub fn build_wt_commandline(project_name: &str, panes: &[PaneSpec], resolved_bash: Option<&Path>) -> String {
    let mut line = String::from("-w -1");
    for (i, p) in panes.iter().enumerate() {
        let title = if i == 0 { project_name } else { &p.title };
        let dir = p.work_dir.display().to_string();
        if i == 0 {
            line.push_str(&format!(" nt -d {} --title {}", quote_wt_arg(&dir), quote_wt_arg(title)));
        } else {
            line.push_str(&format!(" ; sp -V -d {} --title {}", quote_wt_arg(&dir), quote_wt_arg(title)));
        }
        line.push_str(" --suppressApplicationTitle");
        match p.shell {
            Shell::Cmd => {
                line.push_str(" cmd /K \"");
                line.push_str(&cmd_pane_command(&p.work_dir, &p.command));
                line.push('"');
            }
            Shell::PowerShell => {
                line.push_str(" powershell -NoExit -ExecutionPolicy Bypass -EncodedCommand ");
                line.push_str(&encode_ps_command(&ps_pane_script(&p.work_dir, &p.command)));
            }
            Shell::Bash => {
                let bash = resolved_bash.unwrap_or_else(|| Path::new("bash"));
                line.push(' ');
                line.push_str(&quote_wt_arg(&bash.display().to_string()));
                line.push(' ');
                line.push_str(&bash_launch_args(&p.work_dir, &p.command));
            }
        }
    }
    line
}

pub fn cmd_launch_args(work_dir: &Path, command: &str) -> String {
    format!("/K \"{}\"", cmd_pane_command(work_dir, command))
}

pub fn ps_launch_args(work_dir: &Path, command: &str) -> String {
    format!(
        "-NoExit -ExecutionPolicy Bypass -EncodedCommand {}",
        encode_ps_command(&ps_pane_script(work_dir, command))
    )
}

pub fn validate_wt_commandline(line: &str) -> Result<(), String> {
    if line.encode_utf16().count() > MAX_COMMANDLINE_UTF16 {
        Err("启动项命令总长度超过 30000 字符，请拆分启动项".into())
    } else {
        Ok(())
    }
}

fn commandline_utf16_len(program: &str, args: &str) -> usize {
    program.encode_utf16().count() + 1 + args.encode_utf16().count()
}

pub fn plan_spawn(
    resolved_wt: Option<&Path>,
    resolved_bash: Option<&Path>,
    project_name: &str,
    panes: &[PaneSpec],
) -> Result<SpawnPlan, String> {
    if panes.is_empty() {
        return Err("没有可启动的启动项".into());
    }
    if let Some(p) = panes.iter().find(|p| p.shell == Shell::Bash) {
        if resolved_bash.is_none() {
            return Err(format!(
                "启动项「{}」需要 Git Bash，但未找到；请安装 Git for Windows 或设置 DEVLAUNCH_BASH_PATH",
                p.title
            ));
        }
    }
    match resolved_wt {
        Some(wt) => {
            let args = build_wt_commandline(project_name, panes, resolved_bash);
            validate_wt_commandline(&args)?;
            Ok(SpawnPlan::Wt { program: wt.to_path_buf(), args })
        }
        None => {
            let mut launches = Vec::with_capacity(panes.len());
            for p in panes {
                let launch = match p.shell {
                    Shell::Cmd => FallbackLaunch {
                        program: "cmd".to_string(),
                        args: cmd_launch_args(&p.work_dir, &p.command),
                        work_dir: p.work_dir.clone(),
                    },
                    Shell::PowerShell => FallbackLaunch {
                        program: "powershell".to_string(),
                        args: ps_launch_args(&p.work_dir, &p.command),
                        work_dir: p.work_dir.clone(),
                    },
                    Shell::Bash => FallbackLaunch {
                        program: resolved_bash.unwrap_or_else(|| Path::new("bash")).display().to_string(),
                        args: bash_launch_args(&p.work_dir, &p.command),
                        work_dir: p.work_dir.clone(),
                    },
                };
                if commandline_utf16_len(&launch.program, &launch.args) > MAX_COMMANDLINE_UTF16 {
                    return Err(format!("启动项「{}」命令过长，请拆分启动项", p.title));
                }
                launches.push(launch);
            }
            Ok(SpawnPlan::Fallback { launches })
        }
    }
}

pub fn spawn_panes(project_name: &str, panes: &[PaneSpec]) -> std::io::Result<LaunchMode> {
    let resolved = resolve_wt_path();
    let resolved_bash = resolve_bash_path();
    match plan_spawn(resolved.as_deref(), resolved_bash.as_deref(), project_name, panes) {
        Ok(SpawnPlan::Wt { program, args }) => {
            Command::new(program).raw_arg(args).spawn()?;
            Ok(LaunchMode::WindowsTerminal)
        }
        Ok(SpawnPlan::Fallback { launches }) => {
            for (i, launch) in launches.into_iter().enumerate() {
                let spawned = Command::new(launch.program)
                    .raw_arg(launch.args)
                    .current_dir(launch.work_dir)
                    .creation_flags(CREATE_NEW_CONSOLE)
                    .spawn();
                if let Err(e) = spawned {
                    let msg = if i == 0 {
                        e.to_string()
                    } else {
                        format!("已打开 {i} 个终端窗口后失败：{e}")
                    };
                    return Err(std::io::Error::new(e.kind(), msg));
                }
            }
            Ok(LaunchMode::Fallback)
        }
        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Shell;
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    fn pane(title: &str, dir: &str, shell: Shell, cmd: &str) -> PaneSpec {
        PaneSpec { title: title.into(), work_dir: PathBuf::from(dir), shell, command: cmd.into() }
    }

    fn decode_ps_command(b64: &str) -> String {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).unwrap()
    }

    fn fake_wt(dir: &Path) -> PathBuf {
        let p = dir.join("wt.exe");
        std::fs::write(&p, "x").unwrap();
        p
    }

    #[test]
    fn escape_echo_text_escapes_cmd_metachars() {
        assert_eq!(escape_echo_text(r#"a & b | c < d > e ^ f"#), r#"a ^& b ^| c ^< d ^> e ^^ f"#);
        assert_eq!(escape_echo_text("echo 中文"), "echo 中文");
    }

    #[test]
    fn cmd_pane_command_echoes_prompt_per_line() {
        assert_eq!(
            cmd_pane_command(Path::new(r"D:\My Proj\backend"), "conda activate x\npython -m uvicorn main:app"),
            r#"cd /d "D:\My Proj\backend" && echo D:\My Proj\backend^>conda activate x && conda activate x && echo D:\My Proj\backend^>python -m uvicorn main:app && python -m uvicorn main:app"#
        );
        assert_eq!(
            cmd_pane_command(Path::new(r"D:\cxdownload\迅雷"), "dir\ndir"),
            r#"cd /d "D:\cxdownload\迅雷" && echo D:\cxdownload\迅雷^>dir && dir && echo D:\cxdownload\迅雷^>dir && dir"#
        );
    }

    #[test]
    fn cmd_pane_command_skips_blank_lines_and_empty_command() {
        assert_eq!(cmd_pane_command(Path::new(r"D:\p"), "  "), r#"cd /d "D:\p""#);
        assert_eq!(
            cmd_pane_command(Path::new(r"D:\p"), "dir\n\n  \ncd x"),
            r#"cd /d "D:\p" && echo D:\p^>dir && dir && echo D:\p^>cd x && cd x"#
        );
    }

    #[test]
    fn cmd_pane_prompt_for_drive_root() {
        assert_eq!(
            cmd_pane_command(Path::new(r"D:\"), "dir"),
            r#"cd /d "D:\" && echo D:\^>dir && dir"#
        );
    }

    #[test]
    fn ps_pane_script_quotes_and_multiline() {
        assert_eq!(
            ps_pane_script(Path::new(r"D:\it's"), "npm run dev\nnpm test"),
            "Set-Location -LiteralPath 'D:\\it''s'\r\nnpm run dev\nnpm test"
        );
    }

    #[test]
    fn ps_encoded_command_is_utf16le_base64() {
        assert_eq!(encode_ps_command("hi"), "aABpAA==");
    }

    #[test]
    fn wt_commandline_first_tab_then_splits() {
        let panes = vec![
            pane("后端", r"D:\p\backend", Shell::Cmd, "python app.py"),
            pane("前端", r"D:\p\frontend", Shell::PowerShell, "npm run dev"),
        ];
        let line = build_wt_commandline("XingTu", &panes, None);
        assert_eq!(
            line,
            format!(
                r#"-w -1 nt -d "D:\p\backend" --title "XingTu" --suppressApplicationTitle cmd /K "cd /d "D:\p\backend" && echo D:\p\backend^>python app.py && python app.py" ; sp -V -d "D:\p\frontend" --title "前端" --suppressApplicationTitle powershell -NoExit -ExecutionPolicy Bypass -EncodedCommand {}"#,
                encode_ps_command(&ps_pane_script(Path::new(r"D:\p\frontend"), "npm run dev"))
            )
        );
    }

    #[test]
    fn escape_matrix_in_wt_commandline_and_pane_commands() {
        // spec §4 转义矩阵：命令文本在 cmd / PS 两条链路上都必须原样保留（或正确编码）。
        let cases: &[(&str, &str)] = &[
            ("quote", r#"echo "a b""#),
            ("single-quote", "echo 'x'"),
            ("amp", "a & b"),
            ("pipe", "a | b"),
            ("lt", "a < b"),
            ("gt", "a > b"),
            ("caret", "a ^ b"),
            ("percent", "echo %PATH%"),
            ("bang", "echo !x!"),
            ("parens", "echo (x)"),
            ("hash", "rem # hash"),
            ("chinese", "echo 中文"),
            ("emoji", "echo 😀"),
            ("double-amp", "a && b"),
            ("flag", r#"--flag="a b""#),
        ];
        for (label, cmd) in cases {
            let cpane = pane("t", r"D:\p", Shell::Cmd, cmd);
            let line = build_wt_commandline("X", &[cpane], None);
            let expected_pane = format!(
                r#"cd /d "D:\p" && echo {} && {cmd}"#,
                escape_echo_text(&format!("D:\\p>{cmd}"))
            );
            assert_eq!(
                line,
                format!(r#"-w -1 nt -d "D:\p" --title "X" --suppressApplicationTitle cmd /K "{expected_pane}""#),
                "wt cmd case {label}"
            );
            assert_eq!(cmd_pane_command(Path::new(r"D:\p"), cmd), expected_pane, "cmd_pane case {label}");

            let ppane = pane("t", r"D:\p", Shell::PowerShell, cmd);
            let line = build_wt_commandline("X", &[ppane], None);
            assert_eq!(
                line,
                format!(
                    r#"-w -1 nt -d "D:\p" --title "X" --suppressApplicationTitle powershell -NoExit -ExecutionPolicy Bypass -EncodedCommand {}"#,
                    encode_ps_command(&ps_pane_script(Path::new(r"D:\p"), cmd))
                ),
                "wt ps case {label}"
            );
            let b64 = line.rsplit(' ').next().unwrap();
            assert_eq!(
                decode_ps_command(b64),
                format!("Set-Location -LiteralPath 'D:\\p'\r\n{cmd}"),
                "ps script case {label}"
            );
        }
    }

    #[test]
    fn escape_matrix_path_with_spaces() {
        let cpane = pane("t", r"D:\My Proj", Shell::Cmd, "npm run dev");
        assert_eq!(
            build_wt_commandline("X", &[cpane], None),
            r#"-w -1 nt -d "D:\My Proj" --title "X" --suppressApplicationTitle cmd /K "cd /d "D:\My Proj" && echo D:\My Proj^>npm run dev && npm run dev""#
        );
    }

    #[test]
    fn wt_args_escape_trailing_backslash() {
        let panes = vec![pane("t", r"D:\", Shell::Cmd, "echo hi")];
        let line = build_wt_commandline("X\\", &panes, None);
        assert!(line.contains(r#"-d "D:\\" --title "X\\""#), "{line}");
    }

    #[test]
    fn wt_title_replaces_quotes_and_escapes_trailing_backslash() {
        let panes = vec![pane("t", r"D:\p", Shell::Cmd, "echo hi")];
        let line = build_wt_commandline("a\"b\\", &panes, None);
        assert!(line.contains(r#"--title "a'b\\""#), "{line}");
        assert!(!line.contains(r#"--title "a"b"#), "{line}");
    }

    #[test]
    fn plan_spawn_fallback_rejects_overlong_commandline() {
        let panes = vec![pane("t", r"D:\p", Shell::Cmd, &"a".repeat(40_000))];
        let err = plan_spawn(None, None, "X", &panes).unwrap_err();
        assert!(err.contains("过长") || err.contains("30000"), "{err}");
    }

    #[test]
    fn launch_args_for_fallback_windows() {
        assert_eq!(
            cmd_launch_args(Path::new(r"D:\p"), "npm run dev"),
            r#"/K "cd /d "D:\p" && echo D:\p^>npm run dev && npm run dev""#
        );
        let args = ps_launch_args(Path::new(r"D:\p"), "npm run dev");
        let b64 = args
            .strip_prefix("-NoExit -ExecutionPolicy Bypass -EncodedCommand ")
            .expect("ps fallback args prefix");
        assert_eq!(decode_ps_command(b64), "Set-Location -LiteralPath 'D:\\p'\r\nnpm run dev");
    }

    #[test]
    fn resolve_wt_prefers_env_override() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("wt.exe");
        std::fs::write(&fake, "x").unwrap();
        let got = resolve_wt_with(
            Some(fake.as_os_str()),
            None,
            None,
        );
        assert_eq!(got.as_deref(), Some(fake.as_path()));
    }

    #[test]
    fn resolve_wt_empty_override_forces_none() {
        // 空字符串 = 强制禁用 wt（用于验证降级路径）
        assert_eq!(resolve_wt_with(Some(OsStr::new("")), None, None), None);
    }

    #[test]
    fn resolve_wt_scans_path_entries_in_order() {
        let missing = tempfile::tempdir().unwrap().path().join("gone");
        let dir = tempfile::tempdir().unwrap();
        let wt = fake_wt(dir.path());
        let path_env = std::env::join_paths([missing.as_path(), dir.path()]).unwrap();
        let got = resolve_wt_with(None, Some(path_env.as_os_str()), None);
        assert_eq!(got.as_deref(), Some(wt.as_path()));
    }

    #[test]
    fn resolve_wt_falls_back_to_localappdata_windowsapps() {
        let local = tempfile::tempdir().unwrap();
        let apps = local.path().join("Microsoft").join("WindowsApps");
        std::fs::create_dir_all(&apps).unwrap();
        let wt = fake_wt(&apps);
        let got = resolve_wt_with(None, None, Some(local.path().as_os_str()));
        assert_eq!(got.as_deref(), Some(wt.as_path()));
    }

    #[test]
    fn resolve_wt_none_when_nowhere_found() {
        let path_dir = tempfile::tempdir().unwrap();
        let local = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_wt_with(None, Some(path_dir.path().as_os_str()), Some(local.path().as_os_str())),
            None
        );
    }

    #[test]
    fn rejects_overlong_commandline_by_utf16_units() {
        assert!(validate_wt_commandline(&"a".repeat(30_001)).is_err());
        assert!(validate_wt_commandline("short").is_ok());
        assert!(validate_wt_commandline(&"😀".repeat(15_001)).is_err());
        assert!(validate_wt_commandline(&"😀".repeat(15_000)).is_ok());
    }

    #[test]
    fn plan_spawn_wt_uses_resolved_path_and_commandline() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_wt(dir.path());
        let panes = vec![pane("t", r"D:\p", Shell::Cmd, "echo hi")];
        match plan_spawn(Some(&fake), None, "X", &panes).unwrap() {
            SpawnPlan::Wt { program, args } => {
                assert_eq!(program, fake);
                assert_eq!(args, build_wt_commandline("X", &panes, None));
            }
            SpawnPlan::Fallback { .. } => panic!("expected Wt plan"),
        }
    }

    #[test]
    fn plan_spawn_fallback_builds_one_launch_per_pane() {
        let panes = vec![
            pane("后端", r"D:\p\backend", Shell::Cmd, "python app.py"),
            pane("前端", r"D:\p\frontend", Shell::PowerShell, "npm run dev"),
        ];
        match plan_spawn(None, None, "X", &panes).unwrap() {
            SpawnPlan::Fallback { launches } => {
                assert_eq!(launches.len(), 2);
                assert_eq!(launches[0].program, "cmd");
                assert_eq!(launches[0].args, cmd_launch_args(Path::new(r"D:\p\backend"), "python app.py"));
                assert_eq!(launches[0].work_dir, PathBuf::from(r"D:\p\backend"));
                assert_eq!(launches[1].program, "powershell");
                assert_eq!(launches[1].args, ps_launch_args(Path::new(r"D:\p\frontend"), "npm run dev"));
                assert_eq!(launches[1].work_dir, PathBuf::from(r"D:\p\frontend"));
            }
            SpawnPlan::Wt { .. } => panic!("expected Fallback plan"),
        }
    }

    #[test]
    fn plan_spawn_rejects_empty_panes() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_wt(dir.path());
        assert!(plan_spawn(None, None, "X", &[]).is_err());
        assert!(plan_spawn(Some(&fake), None, "X", &[]).is_err());
    }

    #[test]
    fn plan_spawn_rejects_overlong_commandline() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_wt(dir.path());
        let panes = vec![pane("t", r"D:\p", Shell::Cmd, &"a".repeat(40_000))];
        let err = plan_spawn(Some(&fake), None, "X", &panes).unwrap_err();
        assert!(err.contains("30000"));
    }

    #[test]
    fn to_msys_path_matrix() {
        assert_eq!(to_msys_path(Path::new(r"D:\Projects\foo")), "/d/Projects/foo");
        assert_eq!(to_msys_path(Path::new(r"D:\")), "/d");
        assert_eq!(to_msys_path(Path::new(r"D:\My Proj\a")), "/d/My Proj/a");
        assert_eq!(to_msys_path(Path::new(r"D:/already/forward")), "/d/already/forward");
        assert_eq!(to_msys_path(Path::new(r"\\server\share\dir")), "//server/share/dir");
    }

    #[test]
    fn bash_pane_script_contains_cd_command_and_stays_open() {
        let script = bash_pane_script(Path::new(r"D:\My Proj"), "npm run dev\nnpm test");
        assert!(script.starts_with("cd '/d/My Proj'"), "{script}");
        assert!(script.contains("npm run dev\nnpm test"), "{script}");
        assert!(script.ends_with("exec bash -il"), "{script}");
    }

    #[test]
    fn bash_pane_script_escapes_single_quotes_in_path() {
        let script = bash_pane_script(Path::new(r"D:\it's here"), "echo ok");
        assert!(script.contains(r"cd '/d/it'\''s here'"), "{script}");
    }

    #[test]
    fn bash_pane_script_empty_command_still_stays_open() {
        let script = bash_pane_script(Path::new(r"D:\p"), "   ");
        assert_eq!(script, "cd '/d/p'\nexec bash -il");
    }

    #[test]
    fn bash_pane_script_normalizes_crlf() {
        let script = bash_pane_script(Path::new(r"D:\p"), "echo a\r\necho b");
        assert!(script.contains("echo a\necho b"), "{script}");
        assert!(!script.contains('\r'), "{script}");
    }

    #[test]
    fn encode_bash_script_roundtrips_utf8() {
        use base64::Engine;
        let script = "cd '/d/项目'\necho \"a b\"\nexec bash -il";
        let b64 = encode_bash_script(script);
        let decoded = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), script);
    }

    #[test]
    fn bash_launch_args_wraps_and_hides_user_command() {
        let args = bash_launch_args(Path::new(r"D:\p"), "secret-cmd $(rm)");
        assert!(args.starts_with("-lc \"bash -l <(base64 -d<<<"), "{args}");
        assert!(args.ends_with(")\""), "{args}");
        assert!(!args.contains("secret-cmd"), "{args}");
    }

    #[test]
    fn resolve_bash_prefers_override_file() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("bash.exe");
        std::fs::write(&fake, "x").unwrap();
        let got = resolve_bash_with(Some(fake.as_os_str()), None, None, None);
        assert_eq!(got.as_deref(), Some(fake.as_path()));
    }

    #[test]
    fn resolve_bash_empty_override_forces_none() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("bash.exe");
        std::fs::write(&fake, "x").unwrap();
        assert_eq!(resolve_bash_with(Some(OsStr::new("")), None, None, Some(dir.path().as_os_str())), None);
    }

    #[test]
    fn resolve_bash_falls_back_to_program_files() {
        let dir = tempfile::tempdir().unwrap();
        let bash = dir.path().join("Git").join("bin").join("bash.exe");
        std::fs::create_dir_all(bash.parent().unwrap()).unwrap();
        std::fs::write(&bash, "x").unwrap();
        let got = resolve_bash_with(None, Some(dir.path().as_os_str()), None, None);
        assert_eq!(got.as_deref(), Some(bash.as_path()));
    }

    #[test]
    fn resolve_bash_path_scan_skips_system32_and_windowsapps() {
        let dir = tempfile::tempdir().unwrap();
        let system32 = dir.path().join("System32");
        let winapps = dir.path().join("WindowsApps");
        std::fs::create_dir_all(&system32).unwrap();
        std::fs::create_dir_all(&winapps).unwrap();
        std::fs::write(system32.join("bash.exe"), "x").unwrap();
        std::fs::write(winapps.join("bash.exe"), "x").unwrap();
        let joined = std::env::join_paths([&system32, &winapps]).unwrap();
        assert_eq!(resolve_bash_with(None, None, None, Some(joined.as_os_str())), None);
    }

    #[test]
    fn resolve_bash_path_scan_skips_system32_with_trailing_separator() {
        let dir = tempfile::tempdir().unwrap();
        let system32 = dir.path().join("System32");
        std::fs::create_dir_all(&system32).unwrap();
        std::fs::write(system32.join("bash.exe"), "x").unwrap();
        let entry = format!("{}\\", system32.display());
        assert_eq!(resolve_bash_with(None, None, None, Some(OsStr::new(&entry))), None);
    }

    #[test]
    fn resolve_bash_missing_override_falls_through_to_program_files() {
        let dir = tempfile::tempdir().unwrap();
        let bash = dir.path().join("Git").join("bin").join("bash.exe");
        std::fs::create_dir_all(bash.parent().unwrap()).unwrap();
        std::fs::write(&bash, "x").unwrap();
        let missing = dir.path().join("nope").join("bash.exe");
        let got = resolve_bash_with(Some(missing.as_os_str()), Some(dir.path().as_os_str()), None, None);
        assert_eq!(got.as_deref(), Some(bash.as_path()));
    }

    #[test]
    fn resolve_bash_prefers_program_files_over_x86() {
        let x64 = tempfile::tempdir().unwrap();
        let x86 = tempfile::tempdir().unwrap();
        for root in [x64.path(), x86.path()] {
            let bash = root.join("Git").join("bin").join("bash.exe");
            std::fs::create_dir_all(bash.parent().unwrap()).unwrap();
            std::fs::write(&bash, "x").unwrap();
        }
        let got = resolve_bash_with(None, Some(x64.path().as_os_str()), Some(x86.path().as_os_str()), None);
        assert_eq!(got.as_deref(), Some(x64.path().join("Git").join("bin").join("bash.exe").as_path()));
    }

    #[test]
    fn resolve_bash_path_positive_hit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bash.exe"), "x").unwrap();
        let got = resolve_bash_with(None, None, None, Some(dir.path().as_os_str()));
        assert_eq!(got.as_deref(), Some(dir.path().join("bash.exe").as_path()));
    }

    #[test]
    fn wt_bash_pane_uses_absolute_path_and_hides_command() {
        let panes = vec![pane("bash", r"D:\p", Shell::Bash, "echo hi")];
        let bash = Path::new(r"C:\Program Files\Git\bin\bash.exe");
        let line = build_wt_commandline("X", &panes, Some(bash));
        assert!(line.contains(r#""C:\Program Files\Git\bin\bash.exe" -lc "bash -l <(base64 -d<<<"#), "{line}");
        assert!(!line.contains("echo hi"), "{line}");
    }

    #[test]
    fn plan_spawn_bash_without_resolved_bash_errors() {
        let panes = vec![pane("bash", r"D:\p", Shell::Bash, "echo hi")];
        let err = plan_spawn(None, None, "X", &panes).unwrap_err();
        assert!(err.contains("Git Bash"), "{err}");
    }

    #[test]
    fn plan_spawn_fallback_bash_uses_resolved_path() {
        let panes = vec![pane("bash", r"D:\p", Shell::Bash, "echo hi")];
        let bash = Path::new(r"C:\Git\bin\bash.exe");
        match plan_spawn(None, Some(bash), "X", &panes).unwrap() {
            SpawnPlan::Fallback { launches } => {
                assert_eq!(launches[0].program, r"C:\Git\bin\bash.exe");
                assert!(launches[0].args.contains("base64 -d<<<"));
            }
            other => panic!("expected fallback, got {other:?}"),
        }
    }
}

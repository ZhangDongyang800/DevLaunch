use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

pub fn parse(spec: &str) -> Result<Shortcut, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("快捷键不能为空".into());
    }
    let parts: Vec<&str> = spec.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return Err(format!("快捷键至少需要一个修饰键加一个按键：{spec}"));
    }
    let (mods, key) = parts.split_at(parts.len() - 1);
    let mut modifiers = Modifiers::empty();
    for m in mods {
        let bit = match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Modifiers::CONTROL,
            "alt" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            "win" | "super" | "meta" => Modifiers::SUPER,
            other => return Err(format!("不支持的修饰键：{other}")),
        };
        if modifiers.contains(bit) {
            return Err(format!("修饰键重复：{m}"));
        }
        modifiers |= bit;
    }
    Ok(Shortcut::new(Some(modifiers), parse_code(key[0])?))
}

fn parse_code(key: &str) -> Result<Code, String> {
    let upper = key.to_ascii_uppercase();
    let code = match upper.as_str() {
        "A" => Code::KeyA, "B" => Code::KeyB, "C" => Code::KeyC, "D" => Code::KeyD,
        "E" => Code::KeyE, "F" => Code::KeyF, "G" => Code::KeyG, "H" => Code::KeyH,
        "I" => Code::KeyI, "J" => Code::KeyJ, "K" => Code::KeyK, "L" => Code::KeyL,
        "M" => Code::KeyM, "N" => Code::KeyN, "O" => Code::KeyO, "P" => Code::KeyP,
        "Q" => Code::KeyQ, "R" => Code::KeyR, "S" => Code::KeyS, "T" => Code::KeyT,
        "U" => Code::KeyU, "V" => Code::KeyV, "W" => Code::KeyW, "X" => Code::KeyX,
        "Y" => Code::KeyY, "Z" => Code::KeyZ,
        "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2, "3" => Code::Digit3,
        "4" => Code::Digit4, "5" => Code::Digit5, "6" => Code::Digit6, "7" => Code::Digit7,
        "8" => Code::Digit8, "9" => Code::Digit9,
        "F1" => Code::F1, "F2" => Code::F2, "F3" => Code::F3, "F4" => Code::F4,
        "F5" => Code::F5, "F6" => Code::F6, "F7" => Code::F7, "F8" => Code::F8,
        "F9" => Code::F9, "F10" => Code::F10, "F11" => Code::F11, "F12" => Code::F12,
        "SPACE" => Code::Space,
        other => return Err(format!("不支持的按键：{other}（支持 A-Z、0-9、F1-F12、Space）")),
    };
    Ok(code)
}

pub fn register(app: &AppHandle, spec: &str) -> Result<(), String> {
    let shortcut = parse(spec)?;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(shortcut)
        .map_err(|e| format!("快捷键注册失败（可能已被其他程序占用）：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_canonical_combo() {
        let got = parse("Ctrl+Alt+D").unwrap();
        let want = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyD);
        assert_eq!(got, want);
    }

    #[test]
    fn parse_is_case_insensitive() {
        assert_eq!(parse("ctrl+alt+d").unwrap(), parse("Ctrl+Alt+D").unwrap());
        assert_eq!(parse("CTRL+SHIFT+space").unwrap(), parse("Ctrl+Shift+Space").unwrap());
    }

    #[test]
    fn parse_supports_fkeys_and_digits() {
        assert_eq!(
            parse("Ctrl+Alt+F12").unwrap(),
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::F12)
        );
        assert_eq!(parse("Win+Shift+1").unwrap(), Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Digit1));
    }

    #[test]
    fn parse_rejects_without_modifier() {
        assert!(parse("D").unwrap_err().contains("修饰键"));
        assert!(parse("F5").is_err());
    }

    #[test]
    fn parse_rejects_unknown_key_and_modifier() {
        assert!(parse("Ctrl+Alt+F13").is_err());
        assert!(parse("Ctrl+Alt+Enter").is_err());
        assert!(parse("Foo+D").unwrap_err().contains("修饰键"));
    }

    #[test]
    fn parse_rejects_duplicate_modifier_and_empty() {
        assert!(parse("Ctrl+Ctrl+D").unwrap_err().contains("重复"));
        assert!(parse("   ").unwrap_err().contains("不能为空"));
    }
}

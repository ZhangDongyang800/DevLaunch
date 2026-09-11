# DevLaunch

**One-click launcher for your Windows dev project startup** — configure once, replay your terminal startup commands with a single click.

[English](README.md) | [简体中文](README.zh-CN.md)

![Platform](https://img.shields.io/badge/platform-Windows-0078D4)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB)
![Vue](https://img.shields.io/badge/Vue-3-42B883)
![Rust](https://img.shields.io/badge/Rust-stable-000000)
![License](https://img.shields.io/badge/license-MIT-green)

DevLaunch remembers what you do manually when starting a project: open terminals, `cd` into directories, activate environments, run commands. Configure it once; afterwards a single click on a project card opens a Windows Terminal window with one pane per startup item, all running in parallel.

> It is a one-click replay tool, not a terminal orchestrator: no waiting, no polling, no monitoring. Command failures and crashes stay in the real terminal for you to inspect.

## Features

- **One-click startup**: one project = one window, one pane per item, launched in parallel
- **Real interactive terminals**: multi-line commands run sequentially in the same shell session (activate env → start service); `Ctrl+C` behaves like manual typing
- **Works out of the box**: prefers Windows Terminal; falls back to separate terminal windows when `wt` is unavailable
- **Config travels with the repo**: export `devlaunch.json` into the project root, teammates import it and go
- **Legacy config migration**: v1 / v2 configs and templates migrate automatically
- **Tray resident**: per-project launch from the tray menu, optional launch at login

## Quick Start

**Install**: download the `.msi` or `-setup.exe` from [Releases](../../releases), or build from source:

```bash
npm install
npm run tauri build   # artifacts: src-tauri/target/release/
```

**Usage**:

1. Open DevLaunch (it stays in the tray), create a project and pick its root directory
2. Add startup items: name, working directory, command (multi-line supported), shell (cmd / PowerShell)
3. Click the project card to launch

A startup item is just the command you would type by hand, e.g.:

```jsonc
{
  "version": 3,
  "name": "MyApp",
  "items": [
    { "name": "Backend", "workDir": "backend", "shell": "cmd",
      "command": "conda activate app\npython -m uvicorn main:app --reload" },
    { "name": "Frontend", "workDir": "frontend", "shell": "cmd", "command": "npm run dev" }
  ]
}
```

### Share with your team

Use **Export to project root** in the editor to generate `<project root>\devlaunch.json` (no machine-specific paths). Commit it; teammates who create a project pointing at that directory import it automatically. Use relative `workDir` values (e.g. `backend`).

## Development

```bash
npm run tauri dev     # run in dev mode
npm run build         # frontend type-check + build
cargo test            # Rust unit tests (in src-tauri/, 61 tests)
```

Requirements: Windows 10 / 11, Node.js 20.19+ or 22.12+, Rust (stable, MSVC toolchain).
Runtime config: `%APPDATA%\com.devlaunch.app\config.json`.

## License

[MIT](LICENSE)

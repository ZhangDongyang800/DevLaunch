# DevLaunch

**A tray-resident launcher for your projects.**

Save the steps you repeat every time you start a project:

```
Open a terminal → cd into the project → activate the environment → run the command
```

Configure it once; from then on, just click a project to launch.

[English](README.md) | [简体中文](README.zh-CN.md)

![Platform](https://img.shields.io/badge/platform-Windows-0078D4)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB)
![Vue](https://img.shields.io/badge/Vue-3-42B883)
![Rust](https://img.shields.io/badge/Rust-stable-000000)
![License](https://img.shields.io/badge/license-MIT-green)

## Why I built DevLaunch

Before I start developing, these are the things I repeat every day:

```
cd project
cd backend
conda activate xxx
python -m uvicorn main:app --reload
```

Then I open another terminal:

```
cd frontend
npm run dev
```

The more projects you have, the more tedious these repeated steps become.

**I wanted something that does not change how you develop — it just records these tedious manual steps and replays them with one click.**

Of course, it cannot fix bugs or other development problems for you. It only works well when your project itself is stable; it simply removes the tedious steps for situations where you test frequently.

## Features

- **One-click startup**:

  After you click a project, DevLaunch opens the terminal, enters the right directory, and runs the configured commands.

  No more hunting for project folders or typing startup commands by hand.

- **Config travels with the repo**:

  Export a project's startup config as:

  ```
  devlaunch.json
  ```

  Put it in the project root and commit it to Git.

  When teammates pull the project, DevLaunch recognizes the config directly — no need to re-configure the startup items.

  **How a project starts can be part of the project itself.**

- **Tray resident**: launch any project from the right-click menu, with optional launch at login

## Quick Start

**Install**: download the `.msi` or `-setup.exe` from [Releases](../../releases), or build from source:

```bash
npm install
npm run tauri build   # artifacts: src-tauri/target/release/
```

### Share with your team

Use **Export to project root** in the editor to generate `<project root>\devlaunch.json`, then commit it to the repo. When a teammate opens DevLaunch and picks that directory, the startup config is imported automatically.

## Configuration

Each startup item has:

| Field       | Description                            |
| ----------- | -------------------------------------- |
| Name        | The name of the startup item           |
| Working dir | Where the command runs                 |
| Shell       | `cmd` / PowerShell                     |
| Command     | The command to run, multi-line allowed |

For example:

```
Name: Backend

Working dir: backend

Shell: cmd

Command:
conda activate app
python -m uvicorn main:app --reload
```

## Development

```bash
npm run tauri dev     # run in dev mode
npm run build         # frontend type-check + build
```

Requirements: Windows 10 / 11, Node.js 20.19+ or 22.12+, Rust (stable, MSVC toolchain).
Runtime config: `%APPDATA%\com.devlaunch.app\config.json`.

## License

[MIT](LICENSE)

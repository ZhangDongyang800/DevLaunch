# DevLaunch

**A tray-resident launcher for your dev projects.**

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

While developing, you probably repeat these things every day:

```
cd project
cd backend
conda activate xxx
python -m uvicorn main:app --reload
```

Then you open another terminal:

```
cd frontend
npm run dev
```

The more projects you have, the more tedious these repeated steps become.

**DevLaunch does not change how you develop. It just records these manual steps and replays them with one click.**

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

**Usage**:

1. Open DevLaunch, create a project and pick its root directory
2. Add startup items: name, working directory, command, terminal
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

Use **Export to project root** in the editor to generate `<project root>\devlaunch.json` (no machine-specific paths). Commit it; teammates who create a project pointing at that directory import it automatically.

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

# DevLaunch

**Windows 托盘常驻的开发项目一键启动器**——一次配置，一键重放平时手动敲的终端启动命令。

[English](README.md) | [简体中文](README.zh-CN.md)

![Platform](https://img.shields.io/badge/platform-Windows-0078D4)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB)
![Vue](https://img.shields.io/badge/Vue-3-42B883)
![Rust](https://img.shields.io/badge/Rust-stable-000000)
![License](https://img.shields.io/badge/license-MIT-green)

DevLaunch 记住你启动项目时手动做的事：打开终端、`cd` 进目录、激活环境、执行命令。配置一次，之后点击项目卡片，即可在一个 Windows Terminal 窗口中为每个启动项开一个窗格并并行执行。

> 它是一键重放器，不是终端编排器：不等待、不轮询、不监控，命令失败与服务崩溃都留在真实终端里。

## 特性

- **一键启动**：一个项目 = 一个窗口，每个启动项一个窗格，并行执行
- **真实可交互终端**：多行命令在同一 shell 会话内顺序执行（激活环境 → 启动服务），`Ctrl+C` 与手敲一致
- **开箱可用**：优先 Windows Terminal，未安装时自动降级为多个独立终端窗口
- **配置随仓库走**：导出 `devlaunch.json` 到项目根，队友导入即可运行
- **兼容旧配置**：v1 / v2 配置与模板自动迁移
- **托盘常驻**：右键菜单按项目启动，支持开机自启

## 快速开始

**安装**：从 [Releases](../../releases) 下载 `.msi` 或 `-setup.exe`；也可以从源码构建：

```bash
npm install
npm run tauri build   # 产物：src-tauri/target/release/
```

**使用**：

1. 打开 DevLaunch（常驻托盘），新建项目并选择项目根目录
2. 添加启动项：名称、工作目录、命令（支持多行）、命令方言（cmd / PowerShell）
3. 点击项目卡片启动

一个启动项就是一段平时手敲的命令，例如：

```jsonc
{
  "version": 3,
  "name": "MyApp",
  "items": [
    { "name": "后端", "workDir": "backend", "shell": "cmd",
      "command": "conda activate app\npython -m uvicorn main:app --reload" },
    { "name": "前端", "workDir": "frontend", "shell": "cmd", "command": "npm run dev" }
  ]
}
```

### 团队共享

编辑器「导出到项目根」生成 `<项目根>\devlaunch.json`（不含本机路径），提交进仓库；队友新建项目选择该目录时会自动导入。请使用相对 `workDir`（如 `backend`）。

## 开发

```bash
npm run tauri dev     # 调试运行
npm run build         # 前端类型检查 + 构建
cargo test            # Rust 单测（在 src-tauri/ 下，41 个）
```

要求：Windows 10 / 11、Node.js 20.19+ 或 22.12+、Rust（stable，MSVC 工具链）。
运行时配置：`%APPDATA%\com.devlaunch.app\config.json`。

## 许可证

[MIT](LICENSE)

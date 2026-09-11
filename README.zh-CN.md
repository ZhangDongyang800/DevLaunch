# DevLaunch

**一个托盘常驻的开发项目启动器。**

把你平时启动项目时重复执行的操作保存下来：

```
打开终端 → 进入项目目录 → 激活环境 → 执行命令
```

配置一次，以后只需点击项目即可启动。

[English](README.md) | [简体中文](README.zh-CN.md)

![Platform](https://img.shields.io/badge/platform-Windows-0078D4)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB)
![Vue](https://img.shields.io/badge/Vue-3-42B883)
![Rust](https://img.shields.io/badge/Rust-stable-000000)
![License](https://img.shields.io/badge/license-MIT-green)

## 为什么我要做这个 DevLaunch？

开发过程中，你可能每天都在重复这些事情：

```
cd project
cd backend
conda activate xxx
python -m uvicorn main:app --reload
```

然后再打开另一个终端：

```
cd frontend
npm run dev
```

项目越多，这些重复操作越麻烦。

**DevLaunch 不改变你的开发方式，只是把这些手动操作记下来，然后一键启动。**

## 特性

- **一键启动**：

  点击项目后，DevLaunch 自动打开终端、进入对应目录并执行配置好的命令。

  无需再手动寻找项目目录和输入启动命令。

- **配置随仓库走**：

  可以将项目启动配置导出为：

  ```
  devlaunch.json
  ```

  放在项目根目录并提交到 Git。

  队友拉取项目后，DevLaunch 可以直接识别配置，无需重新配置启动项。

  **项目怎么启动，也可以成为项目的一部分。**

- **托盘常驻**：右键菜单按项目启动，支持开机自启

## 快速开始

**安装**：从 [Releases](../../releases) 下载 `.msi` 或 `-setup.exe`；也可以从源码构建：

```bash
npm install
npm run tauri build   # 产物：src-tauri/target/release/
```

**使用**：

1. 打开 DevLaunch，新建项目并选择项目根目录
2. 添加启动项：名称、工作目录、命令、终端
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

编辑器「导出到项目根」生成 `<项目根>\devlaunch.json`（不含本机路径），提交进仓库；队友新建项目选择该目录时会自动导入。

## 配置方式

每个启动项包含：

| 配置     | 说明                   |
| -------- | ---------------------- |
| 名称     | 启动项名称             |
| 工作目录 | 命令执行的位置         |
| Shell    | `cmd` / PowerShell     |
| 命令     | 要执行的命令，支持多行 |

例如：

```
名称：后端

工作目录：backend

Shell：cmd

命令：
conda activate app
python -m uvicorn main:app --reload
```

## 开发

```bash
npm run tauri dev     # 调试运行
npm run build         # 前端类型检查 + 构建
```

要求：Windows 10 / 11、Node.js 20.19+ 或 22.12+、Rust（stable，MSVC 工具链）。
运行时配置：`%APPDATA%\com.devlaunch.app\config.json`。

## 许可证

[MIT](LICENSE)

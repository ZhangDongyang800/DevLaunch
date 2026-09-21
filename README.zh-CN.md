# DevLaunch

**一个托盘常驻的项目启动器。**

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

开发之前，我每天都在重复这些事情：

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

**我就想能不能做一个东西它不改变你的开发方式，只是把这些繁琐的手动操作记下来，然后一键启动。**

当然它并不能帮你解决开发上的报错或者其他难题，它能很好的运行也是建立在你项目稳定的基础上的，它只是简化了繁琐的步骤，针对频繁测试的情况下。

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

- **全局快捷键**：默认 `Ctrl+Alt+D` 唤起搜索面板，输入即过滤，回车启动项目；按 `→` 钻入项目——已启用环境的仓库会先列出各环境（`⎇ 分支` + 端口），回车直接在该环境里启动，下面才是单个启动项；可在设置中录制修改。

- **收藏与最近使用**：常用项目可收藏置顶，启动过的项目按最近时间排序。

- **四套配色主题**：Signal（黑 + 信号绿，默认）、石墨青、靛紫、琥珀棕，在设置中选择并按本机记忆。主题只替换配色 token——布局、间距、字号完全不变——搜索面板与主窗口共用同一套。

- **按任务开发环境（git worktree）**：对一个项目启用环境后，每个分支一个 worktree，统一创建在你指定的根目录下；可选只复制你列入白名单的本地（未跟踪）文件。启动某个环境时，该项目的所有启动项都在这个 worktree 里执行，并注入 `DEVLAUNCH_WORKTREE`、`DEVLAUNCH_WORKTREE_BRANCH`，以及设了端口起点后该分支稳定分配的一个 `PORT`。端口只是一个环境变量：DevLaunch 不探测、不占用、不监控端口，也不等待服务就绪。

- **Git 页（GitHub Desktop 形态）**：可搜索的仓库选择器（名称 + 路径，最近使用优先）+ `Changes | History` 两个 Tab + 快捷键（`Ctrl+R` 刷新、`Ctrl+F` 搜索历史、`Ctrl+Enter` 提交）。Changes 用文件树展示改动，可暂存 / 取消暂存（含全部）、丢弃已跟踪文件的修改，并以结构化 diff 查看（统一 / 左右对比、行号、隐藏空白），支持提交（含 amend）；二进制文件不再是死路：图片差异可预览两侧版本（PNG/JPEG/GIF/WebP/BMP/ICO/AVIF，单侧 ≤4MB），三种视图为并排、叠加（onion-skin 滑杆）、逐像素差异（标红 + 变化像素数），其余二进制显示格式 · 尺寸 · 字节增减。分支选择器可切换 / 新建 / 重命名 / 删除 / 合并 / 变基，提交图带 branch/tag 标签并区分合并节点。History 支持按提交信息 / 作者搜索；右键提交可复制 SHA、Revert、Cherry-pick、soft/mixed Reset，右键文件可打开或查看其历史。顶栏提供 Fetch / Pull / Push（凭据走 Git Credential Manager，无 force-push），提交框带 Commit & Push。项目行保留状态 chip，点击跳转。

## 快速开始

**安装**：从 [Releases](../../releases) 下载 `.msi` 或 `-setup.exe`；也可以从源码构建：

```bash
npm install
npm run tauri build   # 产物：src-tauri/target/release/
```

### 团队共享

编辑器「导出到项目根」生成 `<项目根>\devlaunch.json`，提交进仓库；队友打开DevLaunch，选择该目录时会自动导入启动配置。

## 配置方式

每个启动项包含：

| 配置     | 说明                   |
| -------- | ---------------------- |
| 名称     | 启动项名称             |
| 工作目录 | 命令执行的位置         |
| Shell    | `cmd` / PowerShell / Git Bash          |
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

### 配置文件格式（`devlaunch.json`）

```jsonc
{
  "version": 4,
  "name": "MyApp",
  "items": [
    {
      "name": "后端",
      "workDir": "backend",          // 相对项目根目录；留空 = 根目录
      "shell": "cmd",                // "cmd" | "powershell" | "bash"
      "command": "conda activate app\npython -m uvicorn main:app --reload"
    }
  ]
}
```

- 启动项 `id` 可以省略，DevLaunch 读取时会自动补齐。
- 启用环境的仓库可以额外携带 `worktree` 段（环境根目录、`copy` 白名单、`portBase`、`portKey`）；本机状态（端口租约、收藏、最近使用、热键、git 路径）不会被导出。
- 导入 `devlaunch.json` 等于信任其中的命令（配置即代码），只导入你信任的文件。

### 让 AI 帮你生成配置

DevLaunch 不联网、也不需要内建 AI：任何编码代理 / 聊天 AI 都能直接写出 `devlaunch.json`。把下面这段提示词和项目一起丢给它：

```text
请阅读本项目的 package.json / pyproject.toml / Cargo.toml / go.mod 等项目文件，为我生成一份 devlaunch.json。
格式：{"version":4,"name":"<项目名>","items":[{"name":"<启动项名>","workDir":"<相对目录，可为空>","shell":"cmd","command":"<按手动操作顺序的多行命令>"}]}
要求：只包含启动开发服务所需的命令；多行命令用 \n 分隔；不要编造不存在的脚本或服务。
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

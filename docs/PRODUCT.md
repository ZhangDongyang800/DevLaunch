# DevLaunch 产品说明

> 本文档面向人类读者与 AI 助手，完整描述 DevLaunch 的产品定义、领域模型与行为契约。代码与本文不一致时，以代码为准并更新本文。

## 1. 产品定义

DevLaunch 是一款 Windows 托盘常驻的桌面工具（Tauri 2），解决开发者每次重启电脑后重复执行「打开项目文件夹 → 开多个终端 → 进目录 → 激活环境 → 敲启动命令」的问题。

**定位：一键重放器，不是终端编排器。** 它记住你平时手动启动项目时打开的那些终端和命令，让你一键重放。**核心价值：一次配置，永久复用。**

## 2. 核心概念

```
AppConfig
├── settings      全局设置（autostart）
└── projects[]    项目
    ├── name      项目名
    ├── rootDir   根目录（绝对路径）
    └── items[]   启动项（顺序 = 窗格顺序）
        ├── name      启动项名（如 后端 / 前端）
        ├── workDir   工作目录（可选；空=rootDir；相对路径相对 rootDir 解析）
        ├── shell     cmd | powershell（默认 cmd）
        └── command   命令；支持多行，多行在同一 shell 会话内按顺序执行
                      （如先 conda activate 再启动服务）
```

一个项目 = 一组启动项。没有分组 / 步骤 / 就绪条件 / 编排层级。

## 3. 关键行为契约

1. **终端是真实、可交互的**：一键启动优先调用 Windows Terminal，**一次 `wt` 调用创建一个窗口，每个启动项一个窗格**（首项 `nt`，其余 `sp -V`），所有窗格默认并行启动。命令执行完窗口保留，用户直接接管打字（AI CLI 工具如 opencode 可用）。
2. **启动项内部是同一 shell 会话**：多行命令按用户手动执行的顺序连续执行——cmd 折叠为 ` && ` 连接（顺带 fail-fast：前一条失败不盲目执行下一条），PowerShell 原样多行；先 `cd` 进入 workDir，再激活环境、启动服务。
3. **Rust 不等待、不轮询、不监控**：spawn 成功后启动流程立即结束，不判断服务是否就绪、不清理进程。命令失败、服务崩溃全部留在真实终端里，由用户查看。
4. **降级**：`wt` 不可用时改为每个启动项一个独立终端窗口（cmd 用 `CREATE_NEW_CONSOLE` + `/K`，PowerShell 用 `-NoExit -EncodedCommand`），并发送降级通知；产品语义不变，仅变多窗口。
5. **失败即通知**：项目 / workDir 目录不存在、命令行超长等同步校验失败 → 系统通知（含原因），不启动；spawn 失败 → 系统通知（含原因），已打开的窗口不受影响。错误绝不静默吞掉。
6. **配置单一写者**：所有配置变更（编辑器保存、导入、托盘）都经后端 `Mutex<AppConfig>` 写盘；前端是纯编辑器。写盘是原子的（temp+rename）；配置损坏时备份为 `*.json.corrupt-<时间戳>` 后回退空配置。
7. **托盘**：常驻图标。左键单击 = 打开管理窗口；右键菜单 = 每项目子菜单（启动 / 打开目录）+ 打开 DevLaunch + 退出。**关闭管理窗口只是隐藏，退出只走托盘。**
8. **打开目录**：独立快捷操作，用资源管理器打开项目根目录。

## 4. 配置文件

位置：`%APPDATA%\com.devlaunch.app\config.json`（Tauri app_data_dir）。支持设置页导入/导出 JSON。

```jsonc
{
  "version": 3,
  "settings": { "autostart": false },
  "projects": [
    {
      "id": "uuid",
      "name": "XingTu",
      "rootDir": "D:\\Projects\\XINGTU",
      "items": [
        {
          "id": "uuid",
          "name": "后端",
          "workDir": "backend",
          "shell": "cmd",
          "command": "conda activate xingtu\npython -m uvicorn main:app --host 0.0.0.0 --port 8081 --reload"
        },
        {
          "id": "uuid",
          "name": "前端",
          "workDir": "frontend",
          "shell": "cmd",
          "command": "npm run dev"
        }
      ]
    }
  ]
}
```

- JSON 字段一律 camelCase（serde + TS 类型 `src/types.ts` 双侧锁定，有 roundtrip 单测）。
- 旧版配置（v1 / v2）在加载时自动迁移：每个分组变成一个启动项（组名沿用），组内步骤合并为同一个多行 `command`（workDir 变化处插入 `cd`），`readyCondition` 丢弃，`windowsterminal` → `cmd`；下次保存落盘为 v3。
- **项目内配置分发**：编辑器可把当前项目导出为项目根下的 `devlaunch.json`（ProjectTemplate v3，无 `rootDir`），也可从该文件导入；新建项目选择目录后，若根目录下存在该文件会自动探测并提示导入。

## 5. 界面（深色控制台设计系统）

- **无边框窗口**：自绘标题栏（DevLaunch 品牌 + 拖拽区 + 最小化/隐藏），关闭窗口=隐藏到托盘。
- **首页（启动台）**：项目卡片，**整卡点击=启动**；卡片内渲染启动项标签（`后端 · 前端`）；卡片操作按钮「打开目录 / 编辑 / 删除」（删除需二次确认）；空状态大 CTA 新建项目。
- **编辑器（启动项列表）**：每行一个启动项——序号、名称、目录选择（从 rootDir 子目录下拉或手动输入相对路径）、多行命令输入（等宽字体）、「高级：命令方言」选择器；每项可上下移动 / 单启（「运行此项」）/ 删除。顶部有根目录选择、项目文件导入 / 导出到项目根、保存（含未保存标记）。
- **设置**：开机自启（自绘 toggle，以 autostart 插件注册表为事实来源）、JSON 导入/导出；页面显示配置文件路径。
- **反馈**：toast 分类型（成功绿边/错误红边）+ 系统通知（后台启动的关键事件）。
- **设计 token**：`src/style.css`——近黑三层背景、信号绿 `--signal`、`--mono: Cascadia Code` 用于命令/路径/名称。

## 6. 明确不做（Non-goals）

- 编排线程与启动状态机、跨进程就绪门控（就绪条件 / 端口轮询 / 进程探测 / 输出匹配）
- 进程监控 / 清理、自动重试、finish 哨兵
- pane 布局配置、跨项目编排
- 内嵌终端（永远调用真实外部终端）
- macOS / Linux 实现（架构预留：非 Windows 返回 Unsupported）
- 自动更新、主题切换

## 7. 技术栈与架构速览

- Tauri 2（Rust 后端）+ Vue 3 + TypeScript + Vite；仅 Windows 可用
- Rust 模块：`config`（v3 模型 + 原子读写 + v1/v2 迁移）、`platform`（wt 解析 / cmd·PS 命令构造 / 一次 spawn / 降级）、`launcher`（校验 + spawn + 系统通知）、`commands`（13 个 IPC 命令）、`tray`（菜单）、`lib.rs`（AppState/插件/关窗拦截）
- 启动链路无等待：`ready.rs` 已删除，Rust 不轮询、不监控
- 前端：`store.ts` 单例配置 ref；`api.ts` IPC 封装；所有变更经 invoke 由 Rust 写盘
- 构建细节、开发命令、环境坑：见 `AGENTS.md`

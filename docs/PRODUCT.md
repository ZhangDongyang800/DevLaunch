# DevLaunch 产品说明

> 本文档面向人类读者与 AI 助手，完整描述 DevLaunch 的产品定义、领域模型与行为契约。代码与本文不一致时，以代码为准并更新本文。

## 1. 产品定义

DevLaunch 是一款 Windows 托盘常驻的桌面工具（Tauri 2），解决开发者每次重启电脑后重复执行「打开项目文件夹 → 开终端 → cd 到目录 → 敲命令 → 再开一个终端 → 重复」的问题。

**核心价值：一次配置，永久复用。** 之后每次只需点击一两个按钮，即可自动打开真实终端窗口、进入正确目录并按顺序执行命令。

## 2. 核心概念

```
AppConfig
├── settings      全局设置（默认就绪超时、预留 autostart 字段）
└── projects[]    项目
    ├── name      项目名
    ├── rootDir   根目录（绝对路径）
    └── groups[]  启动组（简单项目只需一个"默认"组）
        └── steps[] 步骤（组内按顺序执行）
            ├── name           步骤名（仅展示用）
            ├── workDir        工作目录（可选；空=rootDir；相对路径相对 rootDir 解析）
            ├── terminal       cmd | powershell | windowsterminal（默认 cmd）
            ├── command        在终端里执行的命令（如 npm run dev / opencode）
            └── readyCondition 本步完成后、下一步启动前的就绪条件
```

**就绪条件（四种，无输出匹配）**

| type | 语义 | 字段 |
|---|---|---|
| `immediate` | 立即启动下一步（默认） | — |
| `delay` | 固定等待 N 秒 | `seconds` |
| `port` | 轮询 TCP 连接，能连上即就绪 | `port`、`host`（默认 127.0.0.1）、`timeoutSec` |
| `process` | 轮询系统进程表，目标进程名存活即就绪 | `processName`（如 `python.exe`，**不要填终端自身**）、`timeoutSec` |

超时语义：`timeoutSec == 0` 回退全局 `settings.readyTimeoutSec`（默认 30）。

## 3. 关键行为契约

1. **终端是真实、可交互的**：每个步骤新开一个真实终端窗口（cmd 用 `/K`、PowerShell 用 `-NoExit`、wt 用 `-d`），命令执行完窗口保留，用户直接接管打字（AI CLI 工具如 opencode 可用）。命令报错时错误信息留在窗口内可见。
2. **执行顺序**：项目级「启动」= 依序执行所有组；组内步骤顺序执行，每步后按 `readyCondition` 等待再启动下一步；**组与组之间无自动等待逻辑**——用户可在首页/编辑器里逐组手动运行（先开数据库组，确认后手动点后端组）。
3. **失败即中断**：任一步 spawn 失败、目录不存在、就绪超时 → 系统通知（含步骤名与原因）+ 停止后续启动；已打开的窗口不受影响。错误绝不静默吞掉。
4. **配置单一写者**：所有配置变更（编辑器保存、导入、托盘）都经后端 `Mutex<AppConfig>` 写盘；前端是纯编辑器。写盘是原子的（temp+rename）；配置损坏时备份为 `*.json.corrupt-<时间戳>` 后回退空配置。
5. **托盘**：常驻图标。左键单击 = 打开管理窗口；右键菜单 = 每项目子菜单（启动 / 打开目录）+ 打开 DevLaunch + 退出。**关闭管理窗口只是隐藏，退出只走托盘。**
6. **打开目录**：独立快捷操作，用资源管理器打开项目根目录。

## 4. 配置文件

位置：`%APPDATA%\com.devlaunch.app\config.json`（Tauri app_data_dir）。支持设置页导入/导出 JSON。

```jsonc
{
  "version": 1,
  "settings": { "readyTimeoutSec": 30, "autostart": false },
  "projects": [
    {
      "id": "uuid",
      "name": "PVDS",
      "rootDir": "D:\\Projects\\PVDS",
      "groups": [
        {
          "id": "uuid",
          "name": "默认",
          "steps": [
            {
              "id": "uuid",
              "name": "server",
              "workDir": "server",
              "terminal": "powershell",
              "command": "python app.py",
              "readyCondition": { "type": "port", "port": 8000, "host": "127.0.0.1", "timeoutSec": 30 }
            },
            {
              "name": "web",
              "terminal": "powershell",
              "command": "npm run dev",
              "readyCondition": { "type": "delay", "seconds": 3 }
            },
            {
              "name": "ai",
              "terminal": "powershell",
              "command": "opencode",
              "readyCondition": { "type": "immediate" }
            }
          ]
        }
      ]
    }
  ]
}
```

JSON 字段一律 camelCase（serde + TS 类型 `src/types.ts` 双侧锁定，有 roundtrip 单测）。

## 5. 界面（深色控制台设计系统）

- **无边框窗口**：自绘标题栏（DevLaunch 品牌 + 拖拽区 + 最小化/隐藏），关闭窗口=隐藏到托盘。
- **首页（启动台）**：项目卡片，**整卡点击=启动**；卡片内渲染启动管道（`命令 → ⏳就绪门槛 → 命令`）；hover 显示「打开目录/编辑」；空状态大 CTA 新建项目。
- **编辑器（时间线）**：垂直时间线（序号圆点+连线），命令输入为主体（等宽字体），就绪门槛显示为步骤下方的「完成后 → …」提示，就绪参数收进可展开参数条；步骤可上下移动/单步运行/删除。
- **设置**：开机自启（自绘 toggle，以 autostart 插件注册表为事实来源）、默认就绪超时、JSON 导入/导出。
- **反馈**：toast 分类型（成功绿边/错误红边）+ 系统通知（后台启动链的关键事件）。
- **设计 token**：`src/style.css`——近黑三层背景、信号绿 `--signal`、`--mono: Cascadia Code` 用于命令/路径/名称。

## 6. 明确不做（Non-goals）

- 输出匹配（监听终端文本判断就绪）——与「真实可交互终端」冲突，有意砍掉
- 复杂依赖拓扑 / DAG 编排、自动重试、进程监控
- 内嵌终端（永远调用真实外部终端）
- macOS / Linux 实现（架构预留：`platform::spawn` 非 Windows 返回 Unsupported）
- 自动更新、主题切换

## 7. 技术栈与架构速览

- Tauri 2（Rust 后端）+ Vue 3 + TypeScript + Vite；仅 Windows 可用
- Rust 模块：`config`（模型+原子读写）、`platform`（终端抽象）、`ready`（同步轮询）、`launcher`（编排+系统通知）、`commands`（10 个 IPC 命令）、`tray`（菜单）、`lib.rs`（AppState/插件/关窗拦截）
- 前端：`store.ts` 单例配置 ref；`api.ts` IPC 封装；所有变更经 invoke 由 Rust 写盘
- 构建细节、开发命令、环境坑：见 `AGENTS.md`

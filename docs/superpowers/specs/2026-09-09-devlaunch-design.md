# DevLaunch 设计文档

日期：2026-09-09
状态：已确认

## 背景与目标

解决开发者每次重启电脑后，需要反复「打开项目文件夹 → 打开终端 → cd 到目录 → 输入命令 → 再开一个终端 → 重复」的繁琐操作。

目标：**一次配置，永久复用；点击一个按钮，自动打开对应项目的真实终端、进入正确目录并执行命令。**

## 技术方案

- Tauri 2 + Rust 后端
- Vue 3 + TypeScript 前端
- Windows 优先，架构预留 macOS/Linux 扩展
- 托盘常驻 + 管理窗口
- 配置保存为 `%APPDATA%\DevLaunch\config.json`（原子写）
- 支持 JSON 导入/导出
- 调用真实外部终端（cmd / PowerShell / Windows Terminal），不做内嵌终端

## 产品架构

```
┌─────────────────────────────────────────────┐
│                DevLaunch (Tauri 2)           │
│                                             │
│  托盘(常驻)          管理窗口(Vue 3)          │
│  ├ 项目列表+启动      ├ 项目/分组/步骤编辑      │
│  ├ 打开目录          ├ 导入/导出 JSON          │
│  └ 退出              └ 设置(开机自启/超时)     │
│        │                    │ IPC (invoke)   │
│        ▼                    ▼               │
│  ┌────────────── Rust 核心 ─────────────┐   │
│  │ config    配置读写(单写者, 原子落盘)    │   │
│  │ launcher  编排: 开终端→就绪等待→下一步  │   │
│  │ ready     就绪轮询(immediate/delay/   │   │
│  │           port/process)              │   │
│  │ platform  终端抽象 trait + Windows 实现│   │
│  │ tray      托盘菜单构建与事件           │   │
│  │ autostart 开机自启(注册表 Run 键)      │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

## 职责边界

| 模块 | 职责 | 明确不做 |
|---|---|---|
| Rust config | 数据结构、加载/保存（temp+rename 原子写）、导入导出、`Mutex<AppConfig>` 托管状态 | 校验以外无业务逻辑 |
| Rust launcher | 接收启动指令 → 按组、按步顺序调用 platform 开终端 → 每步后按 readyCondition 等待 → 失败/超时发通知并中断 | 不监控进程、不重试、不捕获输出 |
| Rust ready | 四种就绪条件轮询：immediate / delay / port(TcpStream 探测) / process(sysinfo 查进程名) | 不知道终端存在 |
| Rust platform | `Terminal` trait：`(workDir, command)` → `std::process::Command`；Windows 实现 cmd/powershell/wt | 不执行 spawn |
| Rust tray | 从配置构建菜单、配置变更后重建、事件转发 | 不直接读写配置文件 |
| Rust autostart | 写/删 `HKCU\...\CurrentVersion\Run` | — |
| Vue 前端 | 纯编辑器，所有配置变更经 IPC 由 Rust 写盘 | 不直接启动终端 |

**关键决策：配置单一写者。** 托盘、窗口、导入导出全部经 Rust `Mutex<AppConfig>`。

## Windows 终端启动方式

| 终端 | 方式 |
|---|---|
| CMD（默认） | `cmd /K "cd /d {workDir} && {command}"` + `CREATE_NEW_CONSOLE`，`/K` 保留窗口使错误可见 |
| PowerShell | `powershell -NoExit -Command "Set-Location '{workDir}'; {command}"` + `CREATE_NEW_CONSOLE` |
| Windows Terminal | `wt -d {workDir} {shell} /K {command}`，不加 `CREATE_NEW_CONSOLE`（wt 是 GUI 程序）；未安装时系统通知报错 |

平台抽象：`platform::Terminal` trait + `platform/windows.rs`；macOS/Linux 由 cfg 门控留空。

## 配置 JSON 结构

```jsonc
{
  "version": 1,
  "settings": {
    "readyTimeoutSec": 30,
    "autostart": false
  },
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
              "terminal": "cmd",
              "command": "python app.py",
              "readyCondition": { "type": "port", "port": 8000, "timeoutSec": 30 }
            }
          ]
        }
      ]
    }
  ]
}
```

- `workDir` 可选：空 = rootDir；相对路径相对 rootDir 解析（解析在 launcher 做）
- `terminal`: `"cmd"`（默认）| `"powershell"` | `"windowsterminal"`
- `readyCondition` serde tagged enum：Immediate / Delay{seconds} / Port{port,timeoutSec} / Process{processName,timeoutSec}
- Group 可选概念，简单项目一个默认组

## MVP 范围

**做**：托盘（项目列表 + [启动][打开目录]）+ 一键启动全部组 + 每组手动运行 + 4 种就绪条件 + 超时通知 + 管理窗口完整增删改 + 导入/导出 + 开机自启 + 三种终端。

**不做**：输出匹配、依赖拓扑、进程监控/重试、内嵌终端、macOS/Linux 实现、自动更新。

## 已识别风险与对策

1. 本机无 wt.exe → 启动失败转系统通知，不崩溃；UI 标注"需已安装"
2. CMD 引号拼接限制 → command 不含嵌套双引号，编辑器提示；推荐复杂命令用 PowerShell
3. `process` 条件须填目标进程名（如 python.exe）而非 shell 名 → 编辑器提示
4. Tauri 2 菜单不可变 → 配置每次保存后统一重建托盘菜单
5. 关窗≠退出 → 窗口 close 事件拦截为隐藏，退出只走托盘
6. 配置损坏 → 原子写 + 加载失败备份旧文件从空配置启动
7. port 探测：TCP 能连即算就绪

## 实现顺序

1. 脚手架：Tauri 2 + Vue 3 + TS，空窗口 + 托盘跑通
2. config.rs：类型 + 原子读写 + 单元测试
3. platform/：三种终端 Command 构建 + 测试
4. ready.rs：四种就绪轮询 + 测试
5. launcher：编排 + IPC 命令
6. tray：菜单构建/重建/事件
7. 前端：项目列表 → 编辑器 → 导入导出
8. autostart + 设置页
9. 端到端手动冒烟（超时、wt 缺失、目录不存在）

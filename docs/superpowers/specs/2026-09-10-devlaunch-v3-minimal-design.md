# DevLaunch v3 设计文档（最小架构）：启动项直启

日期：2026-09-10
状态：待用户复核
取代：`2026-09-10-devlaunch-v3-panes-design.md`（wt pane 编排器，已废弃）
关联：`docs/PRODUCT.md`

## 1. 背景与结论

产品目标：一个按钮，一键启动一个开发项目，替代用户手动「开多个终端 → 进目录 → 激活环境 → 执行启动命令」的繁琐过程。

产品定位：**DevLaunch 是「一键重放器」，不是「终端编排器」**——记住用户平时启动项目时手动打开的多个终端和命令，并让用户一键重放。

对 v3「wt pane 编排器」方案的重新审查结论：其中大部分机制（Rust pane 编排、5 种就绪门、finish 哨兵、process 探测、prelude）是为「跨进程就绪依赖」这一**低频需求**引入的技术派生复杂度，不属于用户核心能力，还引入了新的失败模式（假就绪、超时中断、关窗重建）。故推翻重设计。

最小架构一句话：**项目 = 一组启动项；每个启动项 = 一个真实终端里的一组顺序命令；一键 = 一次 wt 调用开出全部窗格。**

## 2. 产品模型

```jsonc
{
  "version": 3,
  "settings": { "autostart": false },
  "projects": [{
    "id": "uuid", "name": "XingTu", "rootDir": "D:\\Projects\\XINGTU",
    "items": [
      { "id": "uuid", "name": "后端", "workDir": "backend", "shell": "cmd",
        "command": "conda activate xingtu\npython -m uvicorn main:app --host 0.0.0.0 --port 8081 --reload" },
      { "id": "uuid", "name": "前端", "workDir": "frontend", "shell": "cmd",
        "command": "npm run dev" }
    ]
  }]
}
```

- `items[]` 顺序 = 窗格顺序；用户可见术语「启动项」。
- `workDir` 可选：空 = rootDir；相对路径相对 rootDir 解析。
- `shell` 可选：`cmd`（默认）| `powershell`；`windowsterminal` 删除。
- `command` 多行 = 同一 shell 内按序执行（激活、启动自然衔接）。
- 删除概念：`group`、`step`、`readyCondition`、`prelude`、pane。
- `settings` 仅保留 `autostart`；`readyTimeoutSec` 删除（旧配置中的该字段被 serde 忽略）。
- 字段一律 camelCase，serde / TS 双侧锁定（延续既有约定）。

## 3. 启动逻辑（行为契约）

1. 校验项目与每个启动项的 `workDir` 存在。
2. 为每个启动项构造真实 Terminal 命令（cmd / PowerShell 方言）。
3. 优先使用 Windows Terminal。
4. 一次 `wt` 调用创建一个窗口，并为每个启动项创建一个 pane。
5. 每个启动项内部命令保持同一个 shell 会话；多行命令按用户手动执行的语义连续执行。
6. 所有启动项默认并行启动（同一 wt 调用中的 pane 同时拉起）。
7. Rust 不等待、不轮询、不监控、不判断服务是否成功。
8. wt spawn 成功后，启动流程立即结束。
9. 命令失败、服务崩溃等情况全部留在真实终端中，由用户查看。
10. wt 不存在时降级为多个独立终端窗口，产品语义保持一致。

实现细节：

- 一条 wt 命令：首项 `nt`，其余 `sp -V`，一次 spawn。

示例（XingTu）：

```
wt -w -1 nt -d "D:\Projects\XINGTU\backend"  --title "后端" --suppressApplicationTitle cmd /K "<命令1>"
   ; sp -V -d "D:\Projects\XINGTU\frontend" --title "前端" --suppressApplicationTitle cmd /K "<命令2>"
```

- Rust 逐参数传（`;` 为独立 token），不经过任何 shell。
- 标题：首标签 `--title <项目名>`、窗格 `--title <启动项名>`，均加 `--suppressApplicationTitle` 防止子程序改标题。
- **降级**：wt 不可用时改为每项独立窗口 spawn——cmd 用 `CREATE_NEW_CONSOLE` + `/K`，PowerShell 用 `-NoExit -EncodedCommand`；语义不变，仅变多窗口，并发送通知说明。
- 单启动项运行：同样的构造，items 取子集（单独窗口）。
- 重复启动 = 新窗口；关闭窗口 = 该窗全部服务结束（与手动操作一致）。
- 系统通知：spawn 成功 →「已启动「X」N 个窗格」；spawn 失败 → 通知原因。

## 4. 命令投递规范

cmd（`shell: "cmd"`）：

- 行处理：trim、去空行。
- 折叠：多行 → ` && ` 连接。语义 = 手敲一行（预展开行为一致，属有意等价）；`&&` 顺带提供 fail-fast：前一条失败不盲目执行下一条。
- 窗格命令：`cd /d "<workDir>" && <折叠后的命令>`，整体作为 `cmd /K` 的参数。

powershell（`shell: "powershell"`）：

- 脚本文本：`Set-Location -LiteralPath '<workDir>'; <原始多行命令>`。
- 启动：`powershell -NoExit -ExecutionPolicy Bypass -EncodedCommand <UTF-16LE → Base64>`；引号零风险，多行原样保留。

转义矩阵（单测必测）：`"`、`'`、`& | < > ^`、`%VAR%`、`!`、括号、`#`、中文、emoji、路径含空格、`&&`、`--flag="a b"`。

兜底预案（不改变用户模型）：若测试矩阵证明个别 cmd 形态经 wt 传递不可靠，启用其一——① 环境变量传输（命令经进程环境传入，窗格侧用固定文本展开）；② 单行临时脚本 `cmd /K call "%TEMP%\devlaunch-item-<id>.cmd"`。

## 5. 失败语义与事件

| 事件 | 行为 |
|---|---|
| 项目 / 目录不存在 | `launch-result` 返回 Some(err)，不启动 |
| wt 不可用 | 降级多窗口启动 + 通知；不算失败 |
| spawn 失败 | 系统通知（项目 / 原因）；不影响已开窗口 |
| 命令自身失败 / 退出 | 留在窗格内可见；启动器不干预（无监控、无重试） |

- `launch-result` 事件载荷不变（`Option<String>`）：null = 启动已受理；Some = 同步校验失败。
- 不再有门控超时、哨兵、进程探测相关语义。

## 6. 迁移 v1/v2 → v3

- `CONFIG_VERSION = 3`；v1 / v2 配置与模板在加载时自动迁移（内存迁移，下次保存落盘）。
- `groups[] → items[]`：每个分组变成一个启动项，名称沿用组名；同一 group 内连续步骤的命令**合并到同一个启动项**（按序拼接为多行 `command`）。
- 若后一步 workDir 与前一步不同，在拼接处插入 `cd /d "<解析后的绝对路径>"`（cmd）或 `Set-Location -LiteralPath '<...>'`（powershell）。
- 激活类步骤（如 `conda activate xingtu`）自然成为 command 的第一行，无需特殊处理。
- **删除已废弃配置字段**：`readyCondition` 全部丢弃、组 / 步骤层级结构删除；`shell` = 组 terminal（v1 取该步 terminal）；`windowsterminal` → `cmd`。
- `ProjectTemplate.version < 3` 导入时执行同一迁移；导出写 v3。

## 7. 项目内配置分发（保留自 v3）

- 项目根 `devlaunch.json` = ProjectTemplate v3（无 rootDir；rootDir = 文件所在目录）。
- 新增 IPC `export_project_file(projectId) -> path`：写 `<rootDir>\devlaunch.json`（前端先确认覆盖）。
- 导入沿用 `read_project_template(path)`；新建项目选目录后探测 `devlaunch.json` 并提示；写入经 save_config（单写者契约不变）。
- `%APPDATA%` 仍是运行时事实源。

## 8. 前端变更

- 首页项目卡：整卡点击 = 一键启动（不变）；管道渲染简化为「启动项 → 终端」。
- 编辑器：启动项列表（名称 / 目录下拉（复用 `list_subdirs`）/ 多行命令 /「高级：shell」）；每项可单启；删除时间线、步骤、门控概念。
- 托盘：项目级「启动 / 打开目录」保持。
- 设置页：移除「默认就绪超时」。
- 帮助文案：多行 = 同一终端顺序执行；等待示例：cmd `timeout /t 5 /nobreak >nul`、PowerShell `Start-Sleep -Seconds 5`。
- 导入 / 导出入口：导出到项目根 / 从项目文件导入。

## 9. 明确不做与延后

不做：编排线程与启动状态机、哨兵文件、跨进程就绪门控、进程监控 / 清理、pane 布局配置、跨项目编排、输出匹配、内嵌终端、macOS / Linux、自动更新。

延后（插入点已想清）：若未来确认需要「等服务就绪再启动别的」，实现为启动项的可选「启动前等待」（在该窗格命令前内联等待块，如端口轮询 one-liner），**不恢复 v3 的编排体系**。

## 10. 测试策略

- 单测（`cargo test`）：cmd 折叠与 `cd` 注入；ps EncodedCommand；一条 wt 命令构造（`nt` / `sp` / `;` / `-d` / `--title`）；降级 spawn 构造；迁移（组 → 项、跨 workDir 插 `cd`、gate 丢弃）；Config v1 / v2 → v3；项目文件 roundtrip。
- 可注入 spawner：测试用记录器验证 wt argv 与降级路径，不需要真 GUI。
- 手工冒烟：一键多窗格（目录 / 激活 / 命令正确）、Ctrl+C、中文 / emoji、重复启动、关闭窗口、缺 wt 降级、v2 配置迁移、项目文件往返。
- 验收门槛：`npm run build` + `cargo test`；全部完成后统一一次 `npm run tauri build`（AGENTS.md 构建纪律）。

## 11. 风险

| 风险 | 对策 |
|---|---|
| `wt → cmd /K` 引号链 | 转义矩阵单测 + 实机冒烟 + §4 兜底预案 |
| wt 行为版本差异 | 实机冒烟；降级路径兜底 |
| 超长命令行（32767 上限） | 启动前长度校验 + 提示拆分启动项 |

## 12. 相对旧 v3 删除的机制（供复核对照）

Rust pane 编排线程、run 名与窗口寻址、5 种 readyCondition 与轮询、finish 哨兵文件、process 名匹配（含 `sysinfo` 依赖）、prelude 字段与迁移启发式、group / step 层级、编排事件语义重设计。

## 13. 实施顺序（writing-plans 细化）

1. config：v3 模型 + v1/v2 迁移 + ProjectTemplate（含单测）
2. platform：cmd / ps 命令构造 + 一条 wt 调用 + 降级 spawn + 可注入 spawner（含单测）
3. launcher / commands：校验、启动、通知、事件
4. 前端：types / 编辑器 / 导入导出 / 设置页
5. 文档同步（PRODUCT.md、AGENTS.md）+ 手工冒烟 + 构建验收

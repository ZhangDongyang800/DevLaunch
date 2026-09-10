# DevLaunch v3 设计文档：wt pane 编排器

日期：2026-09-10
状态：superseded（被 `2026-09-10-devlaunch-v3-minimal-design.md` 取代）
关联：`docs/PRODUCT.md`、`docs/superpowers/specs/2026-09-09-devlaunch-design.md`（v2）

## 1. 背景与问题

v2 模型：一个分组 = 一个终端窗口；组内步骤编译成一份临时脚本（`cd → 命令 → 等待块`），一次 spawn；就绪等待在脚本内轮询。

对抗性审查确认的缺陷：

| # | 缺陷 | 说明 |
|---|---|---|
| 1 | 门控死锁 | 等待块在「命令之后」。长驻命令（uvicorn / npm dev）不返回 → 本步等待块永不执行、后续步骤永不运行；服务器的 port 门在成功路径上是死代码，只在命令秒退时沦为超时探测器 |
| 2 | 多行语义错误 | 多行被 ` && ` 合并产生预展开问题（`set X=1` 后 `echo %X%` 失败）；第 2 行起的 `.cmd`（npm / conda）无 `call` 会吞掉后续所有行 |
| 3 | Ctrl+C 批处理语义 | 长驻交互工具中断时出现 `Terminate batch job (Y/N)?`，与手敲体验不符 |
| 4 | 命令失败不中断 | 契约「失败即中断」只覆盖 spawn / 目录 / 超时，命令非零退出不停止后续步骤 |
| 5 | 假就绪 | `process` 按进程名匹配全系统（旧进程 / 同名进程）；port 可能被僵尸进程占用误判；无进程清理 |
| 6 | 配置与仓库解耦 | 配置在 `%APPDATA%`，团队共享靠手工导出 |

根因：v2 把「一个终端会话的保真」与「步骤级就绪门控」压进同一份**线性批处理脚本**，两者互斥——门控需要并发观察者，而批处理是单线程序列，且批处理语义与交互式手敲并不等价。

## 2. 产品决策（已确认）

1. **窗口拓扑**：一个分组 = 一个 Windows Terminal 窗口，多 pane；硬依赖 `wt`。
2. **命令执行**：每步一个 pane，命令以「人敲入式」直接执行（不生成批处理脚本、不用 `call`、无脚本内等待块）。
3. **配置分发**：项目根 `devlaunch.json`（可提交进 git），GUI 导入 / 导出。

## 3. 目标 / 非目标

目标：

- 门控真实有效：Rust 在 pane 之间轮询就绪，长驻命令不再阻塞门控
- 命令语义等于手敲（Ctrl+C、展开、交互、错误可见）
- 一组一窗口多 pane，各步骤输出互不干扰
- 配置随仓库走；v1 / v2 配置自动迁移

非目标（v3 明确不做）：

- 跨组依赖 / 就绪联动（组间并行）
- 进程清理 / 监控、命令退出码收集
- 实时状态 UI、输出匹配、内嵌终端
- macOS / Linux、pane / tab 布局配置、自动更新

## 4. 架构

```
commands.launch_project (IPC, background thread)
└─ launcher::launch_project
   ├─ 预校验：wt 存在、所有步骤的 workDir 存在
   ├─ for group in groups（每组成独立流水线线程，并行）
   │    for step in steps:
   │      platform::spawn_pane(run_name, step)   // wt CLI
   │      ready::wait(step.readyCondition)       // Rust 轮询
   │      超时 → 通知 + 停止本组剩余步骤
   └─ 汇总通知（成功组数 / 超时组）
```

模块职责：

| 模块 | v3 职责 |
|---|---|
| config | v3 模型、v1/v2→v3 迁移、ProjectTemplate v3、原子读写 |
| platform/windows | wt 路径解析、pane 命令行构造（cmd / powershell）、spawn（可注入 spawner 供测试） |
| ready | Rust 轮询：immediate / delay / port / process / finish |
| launcher | 并行编排、失败语义、通知、事件 |
| commands | IPC（新增项目文件导入导出）、事件语义更新 |
| tray | 不变（配置变更后重建菜单） |
| 前端 | v3 编辑器（shell / prelude / finish）、导入导出入口 |

## 5. 数据模型 v3

```jsonc
{
  "version": 3,
  "settings": { "readyTimeoutSec": 30, "autostart": false },
  "projects": [{
    "id": "uuid", "name": "XingTu", "rootDir": "D:\\Projects\\XINGTU",
    "groups": [{
      "id": "uuid", "name": "后端",
      "prelude": "conda activate xingtu",          // 可选；组内每个 pane 先执行
      "steps": [{
        "id": "uuid", "name": "api", "workDir": "backend",
        "shell": "cmd",                             // cmd | powershell
        "command": "python -m uvicorn main:app --host 0.0.0.0 --port 8081 --reload",
        "readyCondition": { "type": "port", "port": 8081, "host": "127.0.0.1", "timeoutSec": 30 }
      }]
    }]
  }]
}
```

- `Group.terminal` 删除（窗口恒为 wt）；`Step.terminal` → `Step.shell`；`windowsterminal` 迁移为 `cmd`。
- `readyCondition` 新增 `{"type":"finish"}`（无字段）。
- 字段一律 camelCase，serde / TS 双侧锁定（延续 v2 约定）。
- `timeoutSec == 0` 仍回退全局 `settings.readyTimeoutSec`。

## 6. 迁移 v1/v2 → v3

- 每步 `shell` = 组 terminal（v1 无组 terminal 时取该步 terminal；`windowsterminal` → `cmd`）。
- **prelude 提升**：若某步的 command 是「纯激活类单命令」且其后还有步骤，则该步提升为 `Group.prelude` 并移出步骤列表。每组至多提升一条（取首个满足条件者）；多条激活类命令不做特殊处理（保持为普通步骤）。
- 保守匹配（大小写不敏感，允许 `call ` 前缀）：`conda activate <环境名>`；`<路径>\activate[.bat]`（`activate` 前必须是 `\` 或 `/`）；裸 `activate[.bat]`。明确不匹配 `deactivate`。实现以单测锁定边界。
- 被提升步骤的 readyCondition 丢弃（通常是无意义的 delay），文档说明。
- 若组内只有激活类单步（提升后无步骤），不提升，保留为普通步骤。
- `ProjectTemplate.version < 3` 导入时执行同一迁移。
- 迁移在内存完成；下一次保存时落盘为 v3。

## 7. wt 调用规范

- **路径解析**：`DEVLAUNCH_WT_PATH` 环境变量（测试 / 覆盖）→ PATH 中的 `wt` → `%LOCALAPPDATA%\Microsoft\WindowsApps\wt.exe`；都找不到 → 启动失败（通知 + 安装提示）。
- **run 名**：`devlaunch-<project>-<group>-<epoch_ms>`；非 `[A-Za-z0-9-]` 字符替换为 `-`。每次启动唯一 → 重复启动开新窗口，不污染旧窗口。
- **首 pane**（创建窗口）：
  `wt -w <run> -d <workDir> --title <step.name> <shell args…>`
- **后续 pane**（分栏）：
  `wt -w <run> sp -V -d <workDir> --title <step.name> <shell args…>`
- Rust 用 `Command::new(wt).args([...])` 逐参数传递；起始目录用 `-d` 显式给出，命令内仍 `cd` / `Set-Location` 保证确定性。
- 已知边界（文档化）：用户中途关闭窗口后，后续 `sp` 按 wt 语义**重建同名窗口**；MVP 接受，后续可用 UI Automation 做存活检测。
- pane spawn 失败（wt 无法启动 / 返回错误）→ 通知并停止本组剩余步骤。

## 8. 命令投递规范

### cmd（`shell: "cmd"`）

- 组装：`cd /d "<workDir>" && <prelude> && <command>`；prelude 为空则省略。
- command 多行 → 去空行 / trim 后以 ` && ` 折叠（语义 = 手敲同一行；预展开行为与手敲一行完全一致，属有意等价而非缺陷）。
- 启动：`cmd /K "<组装串>"`，作为该 pane 的 commandline；转义在 platform 层完成并有测试矩阵锁定。
- 交互式 `/K` 命令串：`.bat`（conda / npm）走隐式 call 语义，Ctrl+C 干净返回提示符。

### powershell（`shell: "powershell"`）

- 脚本文本：`Set-Location -LiteralPath '<workDir>'; <prelude>; <command>`；多行原样保留。
- 编码：UTF-16LE → Base64 → `powershell -NoExit -ExecutionPolicy Bypass -EncodedCommand <b64>`；引号零风险。

### 引号 / 转义测试矩阵（单测必测）

`"`、`'`、`& | < > ^`、`%VAR%`、`!`、括号、`#`、中文、emoji、路径含空格、`&&` / `;`、`--flag="a b"`。

### 兜底（风险预案，非 MVP 必做）

仅当测试矩阵发现个别 cmd 形态经 `wt → cmd /K` 转义不可靠时，该步退化为单行临时脚本 `%TEMP%\devlaunch-step-<id>.cmd`，pane 执行 `cmd /K call "<script>"`。这是内部实现细节，不改变用户模型。

## 9. 就绪门控与失败语义

轮询（Rust，间隔 250ms）：

| 门 | 语义 |
|---|---|
| immediate | 不等待，直接 spawn 下一步 |
| delay | sleep N 秒 |
| port | `TcpStream::connect_timeout`（host 默认 127.0.0.1；connect timeout 1s） |
| process | 进程名存活（`sysinfo`，去 `.exe`，大小写不敏感）；按名匹配的误判风险文档化 |
| finish | 命令结束：pane 命令行尾部追加哨兵写入，Rust 轮询哨兵文件存在 |

- 哨兵路径：`%TEMP%\devlaunch-step-<stepId>.done`；spawn 前删除残留文件。
- cmd 尾缀：` & echo.> "%TEMP%\devlaunch-step-<id>.done"`（命令无论成败都会写）。
- ps 尾缀：`; New-Item -ItemType File -Force -Path "$env:TEMP\devlaunch-step-<id>.done" | Out-Null`。
- **不做退出码收集**（cmd 需 `/V:ON` + `!errorlevel!`，会吞用户命令中的 `!`，代价大于收益）；因此 finish 门只表示「命令已结束」，无法判断成败，文档化。

失败语义：

| 事件 | 行为 |
|---|---|
| wt 不存在 / 预校验失败（目录缺失） | 启动不发起；`launch-result` 返回 Some(err) |
| pane spawn 失败（运行时） | 系统通知；停止本组剩余步骤；其他组继续 |
| 门超时 | 系统通知（组 / 步骤 / 原因）；停止本组剩余步骤；已开 pane 保留 |
| 用户中途关窗 | 后续 pane 会重建同名窗口（已知行为，文档化） |

- 组间并行，各组成败独立。
- **事件语义**：`launch-result` 在预校验通过、各组流水线启动后立即返回（null = 启动请求已受理）；其后的 spawn / 门控失败只走系统通知。同步错误仍走 `launch-result` 的 Some(err)。

## 10. 项目内配置分发

- 文件名：项目根 `devlaunch.json`；内容 = ProjectTemplate v3（无 rootDir；rootDir = 文件所在目录）。
- 新增 IPC `export_project_file(projectId) -> path`：直接写 `<rootDir>\devlaunch.json`（无对话框；前端先确认覆盖）。
- 导入沿用 `read_project_template(path)`：前端在新建项目选择目录后探测 `<rootDir>\devlaunch.json` 并提示导入；导入经 save_config 统一写入（单写者契约不变）。
- `%APPDATA%` 仍是运行时事实源。

## 11. 前端变更

- `types.ts`：v3（shell、prelude、finish）。
- 编辑器：步骤 shell 选择（cmd / powershell）；组级「环境准备命令（prelude）」输入；门类型新增 finish；校验提示：
  - 「步骤之间不共享 shell 状态；需要激活环境时用组级环境准备命令，或写在同一命令内」
  - 「cmd 多行将折叠为 `&&` 一行，语义等同手敲一行」
- 首页管道渲染适配 5 种门；设置页不变；导入 / 导出入口更新（导出到项目根 / 从项目文件导入）。

## 12. 测试策略

- 单测（`cargo test`）：
  - cmd 组装 / 转义矩阵；ps EncodedCommand 编码与内容
  - 迁移 v1/v2 → v3（prelude 提升、shell 映射；边界：仅激活步、多激活步、`deactivate` 不匹配）
  - 门控轮询（注入 probe / sleep 假实现）：就绪、超时、finish 哨兵
  - wt 参数构造（首 pane / 分栏、run 名清洗、title / `-d`）
  - 项目文件读写 roundtrip
- 可注入 spawner：pane 启动经注入点，测试用记录器验证「先建窗后分栏」与门控顺序，不需要真 GUI。
- 手工冒烟清单（人工验收）：真 wt 分栏 / 命名 / 标题、prelude 生效、超时提示、中途关窗、Ctrl+C、中文 / emoji、重复启动、项目文件往返、v2 配置迁移。
- 验收门槛：`npm run build` + `cargo test`；全部完成后统一一次 `npm run tauri build`（AGENTS.md 构建纪律）。

## 13. 风险与对策

| 风险 | 对策 |
|---|---|
| `wt → cmd /K` 多层引号 | 转义矩阵单测 + 实机冒烟；必要时启用 §8 单步脚本兜底 |
| wt 窗口名复用 / 关闭语义 | 唯一 run 名；文档化重建行为；后续可加 UI Automation 存活检测 |
| 环境继承 | wt 传 commandline 时默认 `--inheritEnvironment`；实现时验证 |
| 团队需安装 wt | 硬依赖已确认；检测 + 安装提示 |
| process 门误判 | 文档化；后续可做 PID 级跟踪 |

## 14. 实施顺序（writing-plans 细化）

1. config：v3 模型 + 迁移 + ProjectTemplate（含单测）
2. platform：wt 解析 + cmd / ps 命令构造 + 可注入 spawner（含单测）
3. ready：Rust 轮询 5 门（含单测）
4. launcher / commands：并行编排、失败语义、事件、通知
5. 前端：types / 编辑器 / 导入导出
6. 文档同步（PRODUCT.md、AGENTS.md）+ 手工冒烟 + 构建验收

## 15. 文档同步清单

- `docs/PRODUCT.md`：核心概念（prelude / shell / finish）、行为契约（事件语义、失败语义、并行组）、non-goals。
- `AGENTS.md`：架构段（launcher / ready / platform 职责变化）、硬性约定（删除脚本 / GBK / call 相关条款；新增 wt / pane / 转义约定）。
- 本 spec 为设计事实来源；代码与文档不一致时更新文档。

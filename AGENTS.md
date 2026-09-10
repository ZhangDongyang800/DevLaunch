# AGENTS.md — DevLaunch

## What this repo is

Windows 托盘常驻的「开发项目一键重放器」：一次配置项目的启动项（名称/目录/命令/方言），点击按钮一次 `wt` 调用打开一个 Windows Terminal 窗口、每个启动项一个窗格并行执行命令；`wt` 缺失时降级为独立终端窗口。Tauri 2 + Rust + Vue 3 + TS。产品说明见 `docs/PRODUCT.md`；设计 spec 见 `docs/superpowers/specs/2026-09-10-devlaunch-v3-minimal-design.md`（`2026-09-10-devlaunch-v3-panes-design.md` 已标注 superseded，别读）。README.md 面向 GitHub 用户，与 `docs/PRODUCT.md` 同源，改产品行为时两处一起更新。

## Environment quirks（必须知道）

- Shell 是 Windows **cmd.exe**。裸 `bash` 会被解析为 WSL 且**不可用**（无 /bin/bash）——绝不要调用 bash。需要 POSIX 脚本时用 `"C:\Program Files\Git\bin\bash.exe"`（仅 superpowers 技能脚本需要）。
- `robocopy /MOVE` 退出码 1–7 是成功，≥8 才是失败；用 `&&` 链接时注意。
- GUI 冒烟测试无法自动化：托盘点击、窗口交互需要人工验证；跑 `npm run tauri dev` 后**必须杀掉进程**，不能留后台。

## Commands

```bash
npm run build          # vue-tsc 类型检查 + vite build（前端验收的唯一门槛）
cargo test             # 在 src-tauri/ 下；41 个单测（config/platform/launcher/commands）
npm run tauri dev      # 运行调试版（主窗口自动显示）
npm run tauri build    # release 构建（~4min）；产物 src-tauri/target/release/devlaunch.exe
                       #   安装包 bundle/msi/*.msi 与 bundle/nsis/*-setup.exe
```

- Tauri 改 Rust 代码后 `tauri dev` 会重编译；改前端热更新。
- 改了 capabilities/tauri.conf.json 后需要重新构建才生效。
- 单测过滤：`cargo test <关键字>`（如 `cargo test migrate`、`cargo test platform`）；前端没有测试框架，`npm run build` 就是前端验收。
- **构建纪律：攒批构建，不要逐次构建。** `npm run tauri build` 每次 ~4min；一个会话内有多项改动时，先完成全部改动并用 `npm run build` + `cargo test`（秒级）验收，最后统一跑一次 `tauri build`。只有"用户需要立即拿到可执行文件验证"时才允许中途构建。

## Architecture（非显而易见的部分）

```
src-tauri/src/
├── config.rs    # 数据模型 + 原子读写（temp+rename）。损坏→备份 *.json.corrupt-<ts>→回默认
│                # CONFIG_VERSION=3：Project{id,name,rootDir,items[]}／Item{id,name,workDir?,shell:cmd|powershell,command}
│                # v1/v2 配置与模板加载时自动迁移：组→启动项、组内步骤合并为一个多行 command
│                # （workDir 变化处插 cd 行）、readyCondition 丢弃、windowsterminal→cmd
├── platform/    # 终端抽象（仅 Windows）
│   └── windows.rs # wt 解析顺序 DEVLAUNCH_WT_PATH → PATH → %LOCALAPPDATA%\Microsoft\WindowsApps\wt.exe
│                # build_wt_commandline：`-w -1`，首项 `nt -d ... --title 项目名`，其余 `; sp -V -d ... --title 项名`
│                # cmd 窗格命令多行折叠 ` && `、`cd /d` 后整体加引号进 `cmd /K "..."`；ps 走 -EncodedCommand（UTF-16LE Base64）
│                # 一次 raw_arg spawn；wt 缺失降级为每项独立窗口（CREATE_NEW_CONSOLE）
├── launcher.rs  # build_panes（校验 items 非空 + workDir 存在）→ platform::spawn_panes → 通知（成功/降级/失败）
├── commands.rs  # 13 个 IPC 命令：get_config/save_config/list_subdirs/launch_project_cmd/launch_item_cmd/open_dir/
│                # export_config_to/import_config_from/export_project/export_project_file/read_project_template/
│                # get_autostart/set_autostart（launch_* 为同步：校验+spawn 完即返回，无事件、无等待）
├── tray.rs      # 托盘菜单从配置构建；菜单 ID 约定 launch:<id>/open:<id>/show/quit；launch 在后台线程
└── lib.rs       # AppState{config: Mutex, path}；插件注册；关窗=hide 不退出
```

**硬性约定（违反会出 bug）：**

1. **配置单一写者**：所有配置变更（前端保存、导入）都经 Rust `Mutex<AppConfig>`；前端是纯编辑器，绝不持有独立持久化状态。
2. **save_config / import_config_from 成功后必须调 `tray::rebuild(app)`**，否则托盘菜单过期。
3. **IPC 命令名与参数**在 `commands.rs`（Rust snake_case 命令名 + camelCase 参数）与 `src/api.ts` 必须逐字一致；Tauri 自动做 camelCase 转换，前端 invoke 参数用 camelCase。
4. **前端插件调用需要 capability 权限**（`src-tauri/capabilities/default.json`）：缺权限**编译不报错、运行时才失败**。dialog 用了 `dialog:default`；自绘标题栏用了 `core:window:allow-minimize/hide/start-dragging/is-maximized/maximize/unmaximize/toggle-maximize`（双击拖拽区最大化也依赖 toggle-maximize 权限）。
5. **CONFIG_VERSION=3**：模型只有 `Project{id,name,rootDir,items[]}` 与 `Item{id,name,workDir?,shell,command}`；`settings` 只有 `autostart`。**不要重新引入 group/step/readyCondition/readyTimeoutSec/pane 配置等字段**；v1/v2 配置与模板靠 `config.rs` 的迁移路径兼容。
6. **一次 wt 调用 = 一个窗口**：`build_wt_commandline` 用 `-w -1`，首项 `nt`、其余 `; sp -V`，每个启动项一个 pane，并行启动。**命令行经 `raw_arg` 整体拼接、不经任何 shell**；cmd 窗格命令是 `cd /d "<dir>" && <多行折叠>` 整体作为 `cmd /K "<...>"` 的参数（引号是 wt 解析的关键，改动必须跑 `cmd_pane_command` 单测 + 实机冒烟）。PowerShell 走 `-EncodedCommand`（UTF-16LE Base64），引号零风险。命令行 >30000 字符报错提示拆分启动项。
7. **wt 缺失降级**：`resolve_wt_path` 为空时改为每项一个独立窗口（`CREATE_NEW_CONSOLE`；cmd `cmd /K`、ps `-NoExit -EncodedCommand`）并通知；`DEVLAUNCH_WT_PATH` 设为**空字符串 = 强制禁用 wt**（用于验证降级路径）。
8. **Rust 不等待、不轮询、不监控**：`ready.rs` 已删除，`launch_*` 命令同步返回。等待请在用户的命令里自己写（cmd `timeout /t 5 /nobreak >nul`；ps `Start-Sleep -Seconds 5`）。**不要重新引入任何门控/轮询/进程探测/事件上报逻辑。**
9. **无边框窗口**：`decorations:false`；拖拽靠 `data-tauri-drag-region`；`body{user-select:none}` 但 input 已恢复 `user-select:text`。

## Frontend conventions

- 设计系统在 `src/style.css`（深色控制台）：近黑三层背景、**信号绿 `--signal`**（启动/成功/焦点）、`--mono: Cascadia Code` 用于命令/路径/项目名/序号，正文 `--sans`。改样式先看 tokens。
- 首页项目卡**整卡点击=启动**；卡片内启动项标签用 `·` 分隔。编辑器是启动项列表（序号+名称+目录下拉+多行命令+shell）；「运行此项」单启前先 `persist()`。
- toast 分类型：`emit('notify', msg, 'err')` 红边，默认绿边。
- store（`src/store.ts`）是模块级单例 `config` ref；`persist()` 深拷贝后 save_config。
- 项目文件分发：编辑器「导出到项目根」写 `<rootDir>\devlaunch.json`（v3 模板、无 rootDir）；「导入」经 `read_project_template` 只读，写入统一走 save_config。

## Testing status

- 有单测（41）：config（serde 大小写/原子写/损坏备份/v1·v2→v3 迁移/无 version 的 v3 探测/ProjectTemplate）、platform（cmd 折叠与 cd、ps 脚本与 EncodedCommand、转义矩阵、wt 命令行构造、plan_spawn 计划、降级启动参数、wt 解析优先级与分支、UTF-16 超长校验）、launcher（build_panes 的 workDir 归一化解析与目录校验）、commands（subdirs/项目导出/导出到项目根/坏 JSON）。
- **无单测**（人工冒烟验收）：真实终端窗口行为（wt 引号链、多窗格并行）、降级路径、tray、前端交互、IPC 全链路。

## Misc

- 应用 identifier `com.devlaunch.app`（构建警告 `.app` 后缀仅影响 macOS 约定，Windows 无碍，勿改——改了会导致用户配置路径迁移）。用户配置在 `%APPDATA%\com.devlaunch.app\config.json`（设置页也显示此路径）。
- exe 实际名是小写 `devlaunch.exe`（productName=DevLaunch 但 exe 小写）。
- 主窗口 `visible:false`：release 构建从托盘启动、不自动显示窗口；**仅 debug 构建在 setup 里 show()**（lib.rs）。跑 `tauri dev` 能看到窗口是靠这个，别删。
- `tauri-plugin-opener` 在 Cargo.toml / package.json / capabilities（`opener:default`）中均未使用（模板遗留，lib.rs 未注册），清理时需三处同删。
- 非 Windows 目标当前**编译不过**（`platform/mod.rs` 的非 Windows stub 里 `LaunchMode` 缺 `PartialEq`）；本产品仅 Windows，别做跨平台构建/交叉编译验证。
- 根目录未跟踪的 `XingTu-devlaunch.json` 是用户个人模板导出：**不要提交、不要删除**。提交按任务精确 `git add`，不要 `git add -A`。
- 远程 `origin`：https://github.com/ZhangDongyang800/DevLaunch（默认分支 `main` = 当前代码；本地 `main` 分支陈旧勿用；`git push` 即发布到公开仓库）。

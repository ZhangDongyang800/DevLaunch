# AGENTS.md — DevLaunch

## What this repo is

Windows 托盘常驻的「开发项目快速启动器」：一次配置项目/分组/步骤，点击按钮自动打开真实终端、进入目录并按就绪条件顺序执行命令。Tauri 2 + Rust + Vue 3 + TS。产品说明见 `docs/PRODUCT.md`，设计 spec 见 `docs/superpowers/specs/2026-09-09-devlaunch-design.md`。README.md 是 Tauri 模板占位，可直接忽略。

## Environment quirks（必须知道）

- Shell 是 Windows **cmd.exe**。裸 `bash` 会被解析为 WSL 且**不可用**（无 /bin/bash）——绝不要调用 bash。需要 POSIX 脚本时用 `"C:\Program Files\Git\bin\bash.exe"`（仅 superpowers 技能脚本需要）。
- `robocopy /MOVE` 退出码 1–7 是成功，≥8 才是失败；用 `&&` 链接时注意。
- GUI 冒烟测试无法自动化：托盘点击、窗口交互需要人工验证；跑 `npm run tauri dev` 后**必须杀掉进程**，不能留后台。

## Commands

```bash
npm run build          # vue-tsc 类型检查 + vite build（前端验收的唯一门槛）
cargo test             # 在 src-tauri/ 下；32 个单测（config/platform/ready/launcher/commands）
npm run tauri dev      # 运行调试版（主窗口自动显示）
npm run tauri build    # release 构建（~4min）；产物 src-tauri/target/release/devlaunch.exe
                       #   安装包 bundle/msi/*.msi 与 bundle/nsis/*-setup.exe
```

- Tauri 改 Rust 代码后 `tauri dev` 会重编译；改前端热更新。
- 改了 capabilities/tauri.conf.json 后需要重新构建才生效。
- **构建纪律：攒批构建，不要逐次构建。** `npm run tauri build` 每次 ~4min；一个会话内有多项改动时，先全部完成并用 `npm run build`（快，秒级）做类型/编译验收，最后统一跑一次 `tauri build`。只有"用户需要立即拿到可执行文件验证"时才允许中途构建。

## Architecture（非显而易见的部分）

```
src-tauri/src/
├── config.rs    # 数据模型 + 原子读写（temp+rename）。损坏→备份 *.json.corrupt-<ts>→回默认
│                # CONFIG_VERSION=2：terminal 是分组属性（一组=一个终端）；v1 配置/导入自动迁移（取组内第一个非 cmd 步骤）
├── platform/    # 终端抽象。spawn_script(terminal, workDir, script)→真实终端窗口执行临时脚本
│   └── windows.rs # cmd/wt 走 `cmd /K call`、powershell 走 `-File`；raw_arg 原样拼接；join_lines 多行合并
├── ready.rs     # 就绪条件→脚本内等待块转译（不再同步轮询）：cmd 借 powershell one-liner，ps 原生代码块；超时 echo+pause 停住
├── launcher.rs  # 编排：build_group_script 把组内步骤(cd/命令/等待块)生成临时脚本，一次 spawn；Rust 不再等待
├── commands.rs  # 13 个 IPC 命令（launch_* 在后台线程，完成 emit "launch-result" err 侧）
│                 # 单项目导入导出：export_project + read_project_template（只读不落盘，前端经 save_config 统一写入）
│                 # 另有 list_subdirs（workDir 子目录选择器）、get/set_autostart（tauri-plugin-autostart）
├── tray.rs      # 托盘菜单从配置构建；菜单 ID 约定 launch:<id>/open:<id>/show/quit
└── lib.rs       # AppState{config: Mutex, path}；插件注册；关窗=hide 不退出
```

**硬性约定（违反会出 bug）：**

1. **配置单一写者**：所有配置变更（前端保存、导入、托盘）都经 Rust `Mutex<AppConfig>`；前端是纯编辑器，绝不持有独立持久化状态。
2. **save_config / import_config_from 成功后必须调 `tray::rebuild(app)`**，否则托盘菜单过期。
3. **IPC 命令名与参数**在 `commands.rs`（Rust snake_case 命令名 + camelCase 参数）与 `src/api.ts` 必须逐字一致；Tauri 自动做 camelCase 转换，前端 invoke 参数用 camelCase。
4. **前端插件调用需要 capability 权限**（`src-tauri/capabilities/default.json`）：缺权限**编译不报错、运行时才失败**。dialog 用了 `dialog:default`；自绘标题栏用了 `core:window:allow-minimize/hide/start-dragging/is-maximized/maximize/unmaximize/toggle-maximize`（双击拖拽区最大化也依赖 toggle-maximize 权限）。
5. **launch-result 事件**载荷是 `Option<String>`（err 侧）：null=成功，Some=错误。App.vue 监听并 toast。
6. **就绪条件**是 serde tagged enum（`{"type":"port",...}`），TS 侧对应判别联合（`src/types.ts`）。字段：`seconds` / `port`+`host`+`timeoutSec` / `processName`+`timeoutSec`。**没有输出匹配**（有意砍掉，勿加）。**v2 语义：等待发生在终端窗口内**（转译为脚本块），Rust 不再同步等待——「未就绪」只出现在终端里，系统通知不再报。
7. **终端命令行用 `raw_arg` 整体拼接**；cmd/powershell 加 `CREATE_NEW_CONSOLE`，wt 不加。**一组=一个终端**：组内步骤生成临时脚本（`%TEMP%\devlaunch-group-<id>.cmd/.ps1`，按组固定名覆盖写、不累积）。**编码：cmd 脚本按 GBK 直写**（encoding_rs），**绝不要写 `chcp 65001`**——实测 chcp 65001 会破坏 conda.bat 激活（RC=3，Activation file missing）；ps1 写 UTF-8 BOM；cmd 含不可 GBK 编码字符时才回退 UTF-8+chcp。`cmd /K call` 或 `-File` 执行；`Step.terminal` 字段仅为 v1 兼容保留，运行时忽略（用 `Group.terminal`）。脚本 echo 消息走 `safe_label` 过滤 `& | < > ( )` 等字符（batch/PS 语法安全）。**cmd 方言的步骤命令必须加 `call ` 前缀**（`launcher.rs::call_prefixed`）：batch 内调另一个 batch（conda.bat/npm.cmd）不加 call 会转移控制权且不返回，吞掉后续步骤——conda/npm 全中招。
8. **无边框窗口**：`decorations:false`；拖拽靠 `data-tauri-drag-region`；`body{user-select:none}` 但 input 已恢复 `user-select:text`。

## Frontend conventions

- 设计系统在 `src/style.css`（深色控制台）：近黑三层背景、**信号绿 `--signal`**（启动/成功/焦点）、`--mono: Cascadia Code` 用于命令/路径/项目名/序号，正文 `--sans`。改样式先看 tokens。
- 首页项目卡**整卡点击=启动**；编辑器是垂直时间线（序号圆点+连线+门槛提示）。
- toast 分类型：`emit('notify', msg, 'err')` 红边，默认绿边。
- store（`src/store.ts`）是模块级单例 `config` ref；`persist()` 深拷贝后 save_config。

## Testing status

- 有单测：config（serde/原子写/损坏备份/v1→v2 迁移/ProjectTemplate）、platform（join_lines/脚本命令行）、ready（等待块转译）、launcher（脚本组装/文件命名/BOM+chcp）、commands（subdirs/项目导出/坏 JSON）。
- **无单测**（人工冒烟验收）：真实终端窗口行为、tray、前端交互、IPC 全链路。

## Misc

- 应用 identifier `com.devlaunch.app`（构建警告 `.app` 后缀仅影响 macOS 约定，Windows 无碍，勿改——改了会导致用户配置路径迁移）。用户配置在 `%APPDATA%\com.devlaunch.app\config.json`（设置页也显示此路径）。
- exe 实际名是小写 `devlaunch.exe`（productName=DevLaunch 但 exe 小写）。
- 主窗口 `visible:false`：release 构建从托盘启动、不自动显示窗口；**仅 debug 构建在 setup 里 show()**（lib.rs）。跑 `tauri dev` 能看到窗口是靠这个，别删。
- `tauri-plugin-opener` 在 Cargo.toml 中未被使用（CTA 模板遗留，lib.rs 未注册），可清理但需同时删 package.json 依赖与 capabilities 中 `opener:default`。
- 开发分支 `feature/devlaunch`，合并到 main 待人工 e2e 验收后进行。

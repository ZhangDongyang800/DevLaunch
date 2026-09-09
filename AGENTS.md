# AGENTS.md — DevLaunch

## What this repo is

Windows 托盘常驻的「开发项目快速启动器」：一次配置项目/分组/步骤，点击按钮自动打开真实终端、进入目录并按就绪条件顺序执行命令。Tauri 2 + Rust + Vue 3 + TS。产品说明见 `docs/PRODUCT.md`，设计 spec 见 `docs/superpowers/specs/2026-09-09-devlaunch-design.md`。

## Environment quirks（必须知道）

- Shell 是 Windows **cmd.exe**。裸 `bash` 会被解析为 WSL 且**不可用**（无 /bin/bash）——绝不要调用 bash。需要 POSIX 脚本时用 `"C:\Program Files\Git\bin\bash.exe"`（仅 superpowers 技能脚本需要）。
- `robocopy /MOVE` 退出码 1–7 是成功，≥8 才是失败；用 `&&` 链接时注意。
- GUI 冒烟测试无法自动化：托盘点击、窗口交互需要人工验证；跑 `npm run tauri dev` 后**必须杀掉进程**，不能留后台。

## Commands

```bash
npm run build          # vue-tsc 类型检查 + vite build（前端验收的唯一门槛）
cargo test             # 在 src-tauri/ 下；15 个单测（config/platform/ready）
npm run tauri dev      # 运行调试版（主窗口自动显示）
npm run tauri build    # release 构建（~4min）；产物 src-tauri/target/release/devlaunch.exe
                       #   安装包 bundle/msi/*.msi 与 bundle/nsis/*-setup.exe
```

- Tauri 改 Rust 代码后 `tauri dev` 会重编译；改前端热更新。
- 改了 capabilities/tauri.conf.json 后需要重新构建才生效。

## Architecture（非显而易见的部分）

```
src-tauri/src/
├── config.rs    # 数据模型 + 原子读写（temp+rename）。损坏→备份 *.json.corrupt-<ts>→回默认
├── platform/    # 终端抽象。spawn(terminal, workDir, command)→真实终端窗口
│   └── windows.rs # cmd/powershell/wt 三种；raw_arg 原样拼接命令行
├── ready.rs     # 同步轮询就绪：immediate/delay/port/process；timeout_sec==0 回退默认
├── launcher.rs  # 编排：组内顺序启动，每步后等就绪；任一步失败→通知+中断链
├── commands.rs  # 10 个 IPC 命令（launch_* 在后台线程，完成 emit "launch-result" err 侧）
├── tray.rs      # 托盘菜单从配置构建；菜单 ID 约定 launch:<id>/open:<id>/show/quit
└── lib.rs       # AppState{config: Mutex, path}；插件注册；关窗=hide 不退出
```

**硬性约定（违反会出 bug）：**

1. **配置单一写者**：所有配置变更（前端保存、导入、托盘）都经 Rust `Mutex<AppConfig>`；前端是纯编辑器，绝不持有独立持久化状态。
2. **save_config / import_config_from 成功后必须调 `tray::rebuild(app)`**，否则托盘菜单过期。
3. **IPC 命令名与参数**在 `commands.rs`（Rust snake_case 命令名 + camelCase 参数）与 `src/api.ts` 必须逐字一致；Tauri 自动做 camelCase 转换，前端 invoke 参数用 camelCase。
4. **前端插件调用需要 capability 权限**（`src-tauri/capabilities/default.json`）：缺权限**编译不报错、运行时才失败**。dialog 用了 `dialog:default`；自绘标题栏用了 `core:window:allow-minimize/hide/start-dragging/is-maximized/maximize/unmaximize/toggle-maximize`（双击拖拽区最大化也依赖 toggle-maximize 权限）。
5. **launch-result 事件**载荷是 `Option<String>`（err 侧）：null=成功，Some=错误。App.vue 监听并 toast。
6. **就绪条件**是 serde tagged enum（`{"type":"port",...}`），TS 侧对应判别联合（`src/types.ts`）。字段：`seconds` / `port`+`host`+`timeoutSec` / `processName`+`timeoutSec`。**没有输出匹配**（有意砍掉，勿加）。
7. **终端命令行用 `raw_arg` 整体拼接**（cmd /K、powershell -NoExit、wt -d 把剩余行当命令行解析）；cmd/powershell 加 `CREATE_NEW_CONSOLE`，wt 不加。cmd 的命令不能含嵌套双引号（已知限制）。
8. **无边框窗口**：`decorations:false`；拖拽靠 `data-tauri-drag-region`；`body{user-select:none}` 但 input 已恢复 `user-select:text`。

## Frontend conventions

- 设计系统在 `src/style.css`（深色控制台）：近黑三层背景、**信号绿 `--signal`**（启动/成功/焦点）、`--mono: Cascadia Code` 用于命令/路径/项目名/序号，正文 `--sans`。改样式先看 tokens。
- 首页项目卡**整卡点击=启动**；编辑器是垂直时间线（序号圆点+连线+门槛提示）。
- toast 分类型：`emit('notify', msg, 'err')` 红边，默认绿边。
- store（`src/store.ts`）是模块级单例 `config` ref；`persist()` 深拷贝后 save_config。

## Testing status

- 有单测：config（serde/原子写/损坏备份）、platform（命令行字符串）、ready（四种条件+超时回退）。
- **无单测**（人工冒烟验收）：launcher 编排、commands、tray、前端交互。

## Misc

- 应用 identifier `com.devlaunch.app`（构建警告 `.app` 后缀仅影响 macOS 约定，Windows 无碍，勿改——改了会导致用户配置路径迁移）。
- exe 实际名是小写 `devlaunch.exe`（productName=DevLaunch 但 exe 小写）。
- `tauri-plugin-opener` 在 Cargo.toml 中未被使用（CTA 模板遗留），可清理但需同时删 package.json 依赖与 capabilities 中 `opener:default`。
- 开发分支 `feature/devlaunch`，合并到 main 待人工 e2e 验收后进行。
- sysinfo 0.33 API：`sys.refresh_processes(ProcessesToUpdate::All, true)`；轮询每次重建 System（1s 间隔下可接受）。

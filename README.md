# Runbook

轻量桌面命令工作台：Tauri 2 + Rust + xterm.js。项目、命令和参数保存在本机 SQLite 数据库，桌面与独立 CLI 共用同一个 Rust 核心。

[下载 Windows 便携版](https://github.com/fishOfOUC/command_terminal/releases/latest) · [快速开始](docs/QUICKSTART.md)

构建后，双击 `dist/Runbook-Desktop.exe` 启动桌面；AI 可以直接运行同目录下的 `runbook.exe`，无需启动 GUI。

- 项目管理：新建、编辑、收藏、归档恢复、删除，校验实际存在的目录。
- 命令管理：新增、编辑、删除、参数预览；其他 CLI 进程的修改在界面空闲时每 2 秒同步。
- 真实终端：PowerShell（优先使用已安装的 `pwsh`），使用标签页新建、切换和关闭会话；命令在当前标签页执行，返回提示符后可继续输入。切换项目保留会话；关闭应用结束所属进程。
- 主题与历史：项目页和工作台共用浅色／深色开关并记住选择，终端同步配色；每个标签保留自己的终端画面和滚动位置，切换时只读取新输出。
- 命令执行：桌面复用当前标签页的常驻 PowerShell，保留当前目录和变量；忙碌或存在未提交输入时不会注入项目命令。停止发送 Ctrl+C；仍未返回时可手动强制结束终端。独立 CLI 的 `command run` 使用一次性 PTY，保留真实退出码和超时管理。Windows 使用 Job Object 管理所属进程树。
- AI 接口：JSON 默认输出、JSON 文件/标准输入、稳定 ID、查询、局部更新、可选版本检查、机器可读 Schema、明确退出码，无交互式提问。

## 使用 CLI

```powershell
.\dist\runbook.exe gui
.\dist\runbook.exe gui ../another-project
.\dist\runbook.exe schema --list
.\dist\runbook.exe schema --op command_create --pretty
.\dist\runbook.exe schema --pretty
.\dist\runbook.exe project add
.\dist\runbook.exe project list
.\dist\runbook.exe command list --project PROJECT_ID
.\dist\runbook.exe command get --project PROJECT_ID --id COMMAND_ID
.\dist\runbook.exe command update --project PROJECT_ID --id COMMAND_ID --if-revision 1 --base 'pnpm run build'
.\dist\runbook.exe command delete --project PROJECT_ID --id COMMAND_ID --if-revision 2
```

`schema --list` 只列出 JSON API 操作名；`schema --op OP` 只返回该操作及必要定义，适合 AI 按需读取。无筛选的 `schema` 保留完整协议。读取 Schema 不访问或创建数据库。

在任意项目目录使用 `runbook project add` 可直接添加当前目录，名称取目录名、描述留空；也支持 `runbook project add PATH`。CLI 未加入 PATH 时使用可执行文件的完整路径。

在任意目录执行 `runbook gui` 可启动桌面并直接打开当前目录对应的项目；`runbook gui PATH` 按调用目录解析相对路径，也接受绝对路径和 `~/`。目标属于已有项目或其子目录时复用该项目，多个项目包含目标时选路径最深的一个；否则以目录名创建项目再打开。已归档的匹配项目会恢复，已有命令保留。目录必须实际存在，不会在磁盘上新建目录。桌面程序需与 CLI 放在同一目录；`--data-dir` 会传给启动的桌面。

在 Windows 为当前用户注册全局 `runbook` 命令（无需管理员权限）：

双击 `scripts\register-cli.cmd`，或在 PowerShell 执行：

```powershell
.\scripts\register-cli.ps1
```

脚本在用户的 `.local\bin` 创建指向 `dist/runbook.exe` 的启动器，必要时添加用户 PATH。源码目录和解压后的便携目录都自带这两个注册脚本；注册后重新打开终端，搬动程序目录后再次注册即可更新路径。启动器保留调用时的工作目录，因此 `runbook project add` 会添加当前目录；更新 `dist/runbook.exe` 后全局命令也随之更新。

完整示例、协议、参数模型及退出码见 [docs/CLI.md](docs/CLI.md)。给 AI 的最短入口见 [AI_GUIDE.md](AI_GUIDE.md)。

项目自带 [runbook-cli Skill](skills/runbook-cli/SKILL.md)，用于指导 Agent 使用 CLI 管理项目、命令与参数。构建后的便携目录也包含 `skills/runbook-cli/SKILL.md`；该文件随项目维护，不会自动注册到全局技能目录。

默认数据库为 `<程序所在目录>\data\runbook.db`，桌面主题、WebView 设置和缓存也保存在该 `data` 目录，不会回退到 C 盘的 AppData。当前便携目录对应 `dist\data`。可以用 `RUNBOOK_DATA_DIR` 显式覆盖；CLI 也支持全局 `--data-dir PATH`。用 `runbook info` 查询实际路径。CLI 与桌面必须选择同一数据目录，才能看到彼此的修改。

首次打开会自动生成 **Runbook** 项目，目录就是程序所在目录，提供“添加项目”和“启动 GUI”两个带目录输入框的日常命令。内置终端可以直接调用同目录的 `runbook`，无需先注册 PATH；复杂的查询、Schema 和 JSON 操作见 AI 文档。默认项目由代码初始化，发布包不携带开发者数据库；解压到其他位置也能看到，携带数据搬家时会更新它的应用路径。后续启动保留用户的编辑与删除。

给普通使用者的说明见 [快速开始](docs/QUICKSTART.md)。复制或搬家时保留整个程序目录及 `data`；首次下载无需任何数据库文件。

## 开发和构建

Windows 需要 Rust MSVC 工具链、Visual Studio C++ 构建工具、Node.js 和 WebView2 Runtime。

```powershell
npm ci
npm run dev
```

生成桌面程序与 CLI 的便携目录：

```powershell
.\build.ps1
```

构建同时生成 `release/Runbook-0.1.0-windows-x64.zip`。压缩包只含程序、日常说明和 AI 文档，不包含 `dist/data`、本机 WebView 缓存或测试库。

只构建 CLI（不需要 Tauri/WebView2）：

```powershell
cargo build --release -p runbook-cli
.\target\release\runbook.exe --help
```

测试与静态检查：

```powershell
cargo test -p runbook-core -p runbook-cli
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

标准 JSON Schema 合约回归（需要 Python 3 和 `jsonschema` 包）可运行 `cargo build --locked -p runbook-cli` 后执行 `python scripts/test-schema.py`。脚本对照实际 CLI 检查类型、长度、参数条件、局部更新、独立 Schema 和按操作查询，所有数据均使用临时目录。

构建后可运行 `npm run test:projects`，在独立数据库和 WebView2 配置中验证项目页交互、CLI 同步、版本冲突恢复与终端会话保留，不使用日常项目数据。

`npm run test:gui -- --dist` 通过便携 CLI 启动真实桌面，验证当前目录创建、相对路径匹配、归档恢复、直接进入工作台和数据库共享。

`npm run test:portable` 解压实际发布包，在不设置数据目录的情况下验证首次默认项目、应用目录下的数据与 WebView 缓存、两条内置命令，以及整个目录搬家后的数据和主题保留。

`npm run test:terminal-state` 验证终端状态和当前标签页执行的前端调用，不启动真实终端。`cargo test -p runbook-core --lib terminal_protocol` 验证提示符协议，并在 Windows 使用无 PTY 的 PowerShell 脚本检查变量、目录、退出码和异常恢复。

`npm run test:ui` 还验证长输出的缓存、切换后的滚动位置和两页主题设置，不启动真实终端会话。

仓库保留两个关键集成测试文件，覆盖共享数据库并发与回滚、CLI 协议、参数引用、真实终端和进程清理。

查看精简的源码目录树：

```powershell
npm run tree -- --depth 4
```

目录命令遵循 `.gitignore`，包含尚未提交的新源码，排除依赖库、编译产物、缓存和本机数据；不会展开目录链接。深度支持 1–10 层，默认最多输出 500 项，可通过 `--limit` 调整。

## 结构

| 目录 | 职责 |
| --- | --- |
| `crates/runbook-core` | 数据模型、校验、事务存储、JSON 协议、参数引用、PTY 与进程生命周期 |
| `crates/runbook-cli` | CLI 参数和标准输入输出适配；不依赖 GUI |
| `src-tauri` | 桌面窗口及 IPC 适配 |
| `frontend` | 项目管理、命令编辑器、终端界面和桌面通信适配器 |
| `docs` | AI 使用协议与架构说明 |
| `skills/runbook-cli` | Runbook CLI 使用 Skill |

终端运行已保存的 Shell 命令，具备当前用户的文件和进程访问能力。命令的增删改查与预览不会执行命令，只有“执行”、交互终端输入或 CLI 的 `command run` 会运行程序。

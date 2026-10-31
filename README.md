# Runbook

轻量桌面命令工作台：Tauri 2 + Rust + xterm.js。项目、命令和参数保存在本机 SQLite 数据库，桌面与独立 CLI 共用同一个 Rust 核心。

构建后，双击 `dist/Runbook-Desktop.exe` 启动桌面；AI 可以直接运行同目录下的 `runbook.exe`，无需启动 GUI。

- 项目管理：新建、编辑、收藏、归档恢复、删除，校验实际存在的目录。
- 命令管理：新增、编辑、删除、参数预览；其他 CLI 进程的修改在界面空闲时每 2 秒同步。
- 真实终端：PowerShell（优先使用已安装的 `pwsh`），使用标签页新建、切换和关闭会话；命令在当前标签页执行，返回提示符后可继续输入。切换项目保留会话；关闭应用结束所属进程。
- 命令执行：桌面复用当前标签页的常驻 PowerShell，保留当前目录和变量；忙碌或存在未提交输入时不会注入项目命令。停止发送 Ctrl+C；仍未返回时可手动强制结束终端。独立 CLI 的 `command run` 使用一次性 PTY，保留真实退出码和超时管理。Windows 使用 Job Object 管理所属进程树。
- AI 接口：JSON 默认输出、JSON 文件/标准输入、稳定 ID、查询、局部更新、可选版本检查、机器可读 Schema、明确退出码，无交互式提问。

## 使用 CLI

```powershell
.\dist\runbook.exe schema --pretty
.\dist\runbook.exe project add
.\dist\runbook.exe project list
.\dist\runbook.exe command list --project PROJECT_ID
.\dist\runbook.exe command get --project PROJECT_ID --id COMMAND_ID
.\dist\runbook.exe command update --project PROJECT_ID --id COMMAND_ID --if-revision 1 --base 'pnpm run build'
.\dist\runbook.exe command delete --project PROJECT_ID --id COMMAND_ID --if-revision 2
```

在任意项目目录使用 `runbook project add` 可直接添加当前目录，名称取目录名、描述留空；也支持 `runbook project add PATH`。CLI 未加入 PATH 时使用可执行文件的完整路径。

在 Windows 为当前用户注册全局 `runbook` 命令（无需管理员权限）：

```powershell
.\scripts\register-cli.ps1
```

脚本在用户的 `.local\bin` 创建指向 `dist/runbook.exe` 的启动器，必要时添加用户 PATH。启动器保留调用时的工作目录，因此 `runbook project add` 会添加当前目录；更新 `dist/runbook.exe` 后全局命令也随之更新。

完整示例、协议、参数模型及退出码见 [docs/CLI.md](docs/CLI.md)。给 AI 的最短入口见 [AI_GUIDE.md](AI_GUIDE.md)。

项目自带 [runbook-cli Skill](skills/runbook-cli/SKILL.md)，用于指导 Agent 使用 CLI 管理项目、命令与参数。构建后的便携目录也包含 `skills/runbook-cli/SKILL.md`；该文件随项目维护，不会自动注册到全局技能目录。

默认数据库为 `%LOCALAPPDATA%\Runbook\runbook.db`，可以用 `RUNBOOK_DATA_DIR` 覆盖；CLI 也支持全局 `--data-dir PATH`。用 `runbook info` 查询实际路径。CLI 与桌面必须选择同一数据目录，才能看到彼此的修改。

首次打开桌面是空项目库，需要添加真实项目目录。前端通过 Tauri 连接本机后端，请使用桌面程序运行。

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

构建后可运行 `npm run test:projects`，在独立数据库和 WebView2 配置中验证项目页交互、CLI 同步、版本冲突恢复与终端会话保留，不使用日常项目数据。

`npm run test:terminal-state` 验证终端状态和当前标签页执行的前端调用，不启动真实终端。`cargo test -p runbook-core --lib terminal_protocol` 验证提示符协议，并在 Windows 使用无 PTY 的 PowerShell 脚本检查变量、目录、退出码和异常恢复。

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

# Runbook 快速开始

解压整个压缩包，双击 `Runbook-Desktop.exe` 即可使用。

首次打开会看到 **Runbook** 项目，路径是程序所在目录。打开它可使用两条日常命令：

- **添加项目**：填写已有的项目目录，点击执行，将它加入项目列表。
- **启动 GUI**：填写要打开的目录，点击执行；已有项目直接打开，没有对应项目时自动添加。目录留空则使用终端当前目录。

也可以在终端运行：

```powershell
runbook project add            # 添加当前目录
runbook project add ..\my-app  # 添加其他目录
runbook gui                    # 打开当前目录对应的项目
runbook gui ..\my-app          # 打开其他目录对应的项目
```

应用内终端可以直接使用 `runbook`。需要在外部终端全局使用时，双击 `scripts\register-cli.cmd` 即可为当前 Windows 用户注册，无需管理员权限；注册后重新打开终端。也可以在 PowerShell 运行 `scripts\register-cli.ps1`，或直接使用 `runbook.exe` 的完整路径。搬动程序目录后，再运行一次注册脚本更新位置。

数据、主题和缓存默认都在程序旁的 `data` 文件夹。不要只移动单个 exe；保留整个目录和 `data`，即可保留自己的项目和设置。应用目录必须可写。

目录必须已存在。“添加项目”只登记项目，不创建或删除磁盘文件。复杂的批量管理和 JSON 接口供 AI 使用，说明在 `AI_GUIDE.md` 与 `docs/CLI.md`。

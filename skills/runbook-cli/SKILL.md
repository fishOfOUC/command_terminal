---
name: runbook-cli
description: 使用 Runbook CLI 管理本地项目、命令库和参数，预览或按用户要求执行已保存的命令。适用于添加当前项目、维护常用命令、处理桌面与 CLI 的并发修改，以及解析 Runbook 的 JSON 结果。
---

# Runbook CLI

Runbook 是本地命令工作台。CLI 与桌面共享 SQLite 数据，通过 CLI 管理项目和命令不需要启动桌面程序。

## 找到程序和数据

- 优先使用用户或当前项目提供的 CLI 路径。源码仓库通常为 `dist/runbook.exe`，仅构建 CLI 时也可能位于 `target/release/runbook.exe`。
- 本 skill 随仓库或便携程序放在 `skills/runbook-cli/`。从这里向上两级是对应根目录：源码仓库的程序在 `dist/runbook.exe`，便携目录的程序在 `runbook.exe`。如果 skill 被单独复制到其他位置，使用已知程序路径或 PATH 中的 `runbook`，不要据此推断软件安装目录。
- 先用 `info` 确认数据库位置，用 `schema` 获取当前版本支持的对象和操作。程序缺失时说明实际缺失路径，不要用浏览器演示数据替代。
- 数据目录优先级为 `--data-dir DIR` > `RUNBOOK_DATA_DIR` > 可执行文件所在目录的 `data`。默认数据库是 `<应用目录>/data/runbook.db`，不会回退到用户的 AppData；桌面设置和缓存也在同一数据目录。桌面与 CLI 必须选择同一目录才能共享数据；不要为解决“看不到项目”而另建一个数据库。
- 首次使用会生成名为 Runbook 的项目，路径是应用目录，只有“添加项目”和“启动 GUI”两个日常命令。它不是用户当前工作目录；操作业务项目时仍按真实路径和 ID 查询。重新打开不会重置用户对默认项目的编辑或删除。

下面的 `$runbook` 表示已确定的可执行文件完整路径。不要在切换工作目录之后继续使用指向原目录的相对路径。

```powershell
# 在源码仓库根目录确定路径；便携目录使用 .\runbook.exe。
$runbook = (Resolve-Path '.\dist\runbook.exe').Path
& $runbook info
& $runbook schema
```

## 添加和查找项目

优先用快捷入口添加本地目录：

```powershell
# 在用户要添加的项目目录执行，使用当前目录。
& $runbook project add

# 也可以指定相对或绝对目录。
& $runbook project add ../another-project
& $runbook project add 'E:\Projects\my-app' --name '我的应用' --description '本地开发'

& $runbook project list --archived false
& $runbook project list --query 'my-app'
& $runbook project get --id PROJECT_ID
```

- `add` 默认使用真实目录名，描述留空，不生成“默认项目”之类的占位名称。目录必须真实存在；默认名称超过 60 个字符时用 `--name` 指定短名称。
- 重复目录（包括已归档项目）返回 `ALREADY_EXISTS`，`error.details.projectId` 是已有项目 ID。读取已有项目，再根据用户意图使用、修改或恢复；不要删除已有项目后重新添加。
- `project list` 默认包含归档项目，活动项目要加 `--archived false`。列表顺序是稳定 ID 顺序，不代表最近打开。
- 需要指定稳定 ID 或提交完整对象时用 `project create --input FILE`。准确字段以 `schema` 为准。

## 管理命令和版本

先查询项目和命令，使用真实 ID；名称和数组位置不能充当更新目标。更新前读取最新 `revision`，只提交需要修改的字段。

```powershell
& $runbook command list --project PROJECT_ID
& $runbook command create --project PROJECT_ID --name '查看工作区' --base 'git status --short'
& $runbook command get --project PROJECT_ID --id COMMAND_ID
& $runbook command update --project PROJECT_ID --id COMMAND_ID --if-revision REVISION --name 'Git 状态'
& $runbook command delete --project PROJECT_ID --id COMMAND_ID --if-revision REVISION
```

`PROJECT_ID`、`COMMAND_ID` 和 `REVISION` 是示意值，实际调用时替换为读取到的 ID 和整数版本号。创建结果中的 ID 位于 `data.id`，版本位于 `data.revision`。

- 项目和命令分别有版本。命令变化也会提高项目版本，桌面打开项目也可能更新项目版本；不要长期缓存版本号。
- `REVISION_CONFLICT` 时重新 `get`，比较最新对象与原始意图，合并仍适用的字段后携带新版本提交。如果同一字段的修改意图发生冲突且无法从用户要求判断，先让用户决定；不要直接去掉版本检查或无限重试。
- `params` 是完整替换：修改参数前读取现有数组，保留未修改项。`params: []` 会清空参数。
- 创建可以提供稳定的 `id`，重复 ID 返回错误，不会隐式覆盖。调用结果不确定时先查询该 ID，再决定是否重试。
- 普通数据操作使用对应子命令，或 `api --input FILE` 提交 `schema` 中的结构化请求。`api` 不接受终端执行操作。

归档、恢复和删除项目使用：

```text
runbook project update --id PROJECT_ID --if-revision REVISION --archived true
runbook project update --id PROJECT_ID --if-revision REVISION --archived false
runbook project delete --id PROJECT_ID --if-revision REVISION
```

删除项目会删除其命令库，磁盘目录和文件不受影响；归档保留命令。正在运行的桌面程序会同步这些变化，并清理已归档或已删除项目的终端会话。

## JSON、参数与执行

- `--input FILE` 读取 UTF-8 JSON，`--input -` 读取 stdin。输入上限为 4 MiB。`--input` 不能与字段模式选项混用；使用 JSON 创建时也把自定义 ID 放进 JSON。
- PowerShell 5.1 的中文管道编码可能影响 stdin；优先使用 UTF-8 JSON 文件。向子进程直接写入 UTF-8 stdin 也可避免 shell 管道编码问题。
- 参数类型为 `text`、`number`、`boolean`。位置参数使用空 `flag`，布尔值使用 JSON 的 `true` / `false`。基础命令放在 `base`，参数按顺序由 Runbook 引用并拼接，不要再自行拼成 shell 字符串。
- `command render` 校验并预览，不执行命令。增删改查也不会执行命令；用户要求执行时再使用 `command run`。

```text
runbook command render --project PROJECT_ID --id COMMAND_ID --values values.json
runbook command run --project PROJECT_ID --id COMMAND_ID --values values.json --if-revision REVISION --timeout-ms 10000
```

`--values` 可省略，此时使用已保存参数值；执行时传入的值不会改写默认值。`run` 默认 30 秒超时，范围为 1–86400000 毫秒，超时会结束所属会话进程。长驻服务通常从桌面启动，以便保留交互和日志。

完整的参数 JSON、API 请求和输入规则见 [CLI 与 AI 协议](../../docs/CLI.md)，按任务需要查阅；当前二进制的 `schema` 和 `--help` 优先于旧文档。

## 解析结果

普通操作的 stdout 是一个 JSON 对象；`--pretty` 只改变排版，`--help` / `--version` 为可读文本。

- 成功：`{"schemaVersion":1,"ok":true,"data":...}`。列表结果是 `data.items` 与 `data.total`。
- 一般失败：`{"schemaVersion":1,"ok":false,"error":{"code":...,"message":...,"details":...}}`。检查退出码和 `ok`，不要凭有输出就认定成功。
- `command run` 的退出码 `10` 或 `124` 仍返回 `data`，并设置 `ok:false`。从 `data.exitCode`、`data.timedOut`、`data.output` 读取实际执行结果，不要只查找 `error`。
- 终端输出可能含 ANSI 控制序列，并可能截断；检查 `data.outputTruncated`，不要把截断日志描述为完整日志。

| CLI 退出码 | 处理含义 |
| --- | --- |
| 0 | 操作成功或被执行命令成功 |
| 1 | 数据库、文件或终端等运行错误 |
| 2 | 参数、JSON 或校验错误，修正输入 |
| 3 | 对象不存在，重新查询 |
| 4 | 重复对象或版本冲突，根据 `error.code` 分别处理 |
| 10 | 被执行命令返回非零退出码，查看 `data` |
| 124 | 执行超时，查看 `data` |

通过 CLI 使用时只走公开协议，不直接修改 SQLite。桌面空闲时约每两秒同步数据；报告“CLI 写入成功”和“桌面已显示”时，应分别依据实际验证结果。

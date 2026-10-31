# CLI 与 AI 协议

CLI 是独立的 Rust 程序，不启动 Tauri、浏览器或 Node.js。下面示例在项目根目录运行。

Windows 可运行 `scripts/register-cli.ps1` 为当前用户注册全局 `runbook` 命令。启动器指向当前仓库的 `dist/runbook.exe`，保留调用目录；如果需要新增 PATH，已有终端需重新打开。也可以继续使用可执行文件的完整路径。

## 创建项目和命令

在项目目录里执行 `runbook project add` 即可添加当前目录：项目名称取实际目录名，描述留空，不需要手填路径或启动桌面程序。也支持相对路径、绝对路径和 `~/`，并可覆盖名称、描述：

```powershell
runbook project add
runbook project add ../another-project
runbook project add 'E:\Projects\my-app' --name '我的应用' --description '本地开发'
```

名称超过 60 个字符时请用 `--name` 指定短名称。盘符根目录使用盘符作为名称。已添加的目录（包括已归档项目）返回 `ALREADY_EXISTS` 和已有项目 ID，不会覆盖或自动恢复项目。未将 CLI 加入 PATH 时，请使用 `dist/runbook.exe` 的完整路径。

```powershell
$cli = '.\dist\runbook.exe'
$project = & $cli project create --name '我的项目' --path (Get-Location).Path | ConvertFrom-Json
$projectId = $project.data.id

$command = & $cli command create --project $projectId --name '查看工作区' --base 'git status --short' | ConvertFrom-Json
$commandId = $command.data.id

& $cli command get --project $projectId --id $commandId
& $cli command update --project $projectId --id $commandId --if-revision 1 --name 'Git 状态'
& $cli command delete --project $projectId --id $commandId --if-revision 2
```

`create` 可以指定稳定的 `id`（在 JSON 输入中，或字段模式的 `--id`）。重复 ID 返回 `ALREADY_EXISTS`，不会隐式覆盖。ID 允许 1–100 个 ASCII 字母、数字、连字符、下划线。命令 ID 在项目内唯一，项目目录也不能重复。

## JSON 输入

把下面对象保存为 UTF-8 的 `command.json`：

```json
{
  "id": "dev-server",
  "name": "启动开发服务",
  "base": "pnpm run dev --",
  "description": "本地前端开发服务器",
  "params": [
    {"id":"port","label":"端口","flag":"--port","type":"number","value":"5173","required":true,"min":1,"max":65535},
    {"id":"host","label":"监听地址","flag":"--host","type":"text","value":"127.0.0.1","required":true},
    {"id":"open","label":"打开浏览器","flag":"--open","type":"boolean","value":false}
  ]
}
```

```powershell
.\dist\runbook.exe command create --project PROJECT_ID --input command.json
```

AI 也可以向子进程 stdin 直接写入该 JSON，使用 `--input -`。输入最大 4 MiB，可接受 UTF-8 BOM。`--input` 与名称、基础命令等字段选项互斥，避免含糊的覆盖顺序。包含 ID 时请将 ID 一并放进 JSON。

在 Windows PowerShell 5.1 中，含中文的管道输入需要先设置 `$OutputEncoding = [System.Text.UTF8Encoding]::new($false)`；使用 UTF-8 JSON 文件可以避免 Shell 管道编码差异。

更新输入是局部对象，例如 `{"base":"pnpm exec vite","description":"直接启动 Vite"}`。省略或为 `null` 的字段保持不变；`params: []` 清空参数，`params: [...]` 替换完整参数数组。`id`、`revision`、时间戳均不能通过更新更改。未知字段会报错，避免拼写错误被忽略。

## 参数和值

- `type` 为 `text`、`number` 或 `boolean`。数字默认值可用 JSON 数字或数字字符串；开关必须是 JSON 布尔值。
- `id` 是传入值时使用的稳定键，`label` 是给人看的标签，`flag` 是 `--port` / `-p` 这样的命令参数。位置参数的 `flag` 为空字符串。
- 参数按数组顺序追加到 `base` 后。开关仅在值为 `true` 时追加对应标记。
- 空的非必填参数会省略，必填校验在执行或 `render` 时进行。创建时允许必填参数的默认值为空，供用户运行时填写。
- `base` 是有意允许 Shell 语法的基础命令；参数值按目标 Shell 的单引号规则作为文字引用，避免空格、美元符号、分号和引号改变参数含义。
- Windows 使用 PowerShell 的引用规则，Unix 使用 POSIX Shell 的引用规则。预览不会启动 Shell。

`values.json` 示例：

```json
{"port": 6001, "host": "127.0.0.1", "open": false}
```

```powershell
.\dist\runbook.exe command render --project PROJECT_ID --id dev-server --values values.json
.\dist\runbook.exe command run --project PROJECT_ID --id dev-server --values values.json --timeout-ms 10000
```

`run` 明确执行命令，默认 30 秒超时（支持 1–86400000 毫秒）。返回 PTY 合并后的 `output`、真实 `exitCode`、`timedOut`、`durationMs` 和 `outputTruncated`。输出可能含 ANSI 终端控制序列，最多保留最后 2 MiB；执行期的参数覆盖不会修改已保存的默认值。长驻开发服务更适合从桌面启动，CLI 超时会结束所属会话进程。

## 单一 JSON API

`runbook api --input -` 接收下面这样的请求：

```json
{
  "op": "command_update",
  "project": "PROJECT_ID",
  "id": "COMMAND_ID",
  "ifRevision": 3,
  "patch": {"name": "构建并检查", "base": "pnpm run build"}
}
```

操作有 `info`、`schema`、`project_list/get/create/update/delete/touch`、`command_list/get/create/update/delete/render`。准确的字段定义以当前程序的 `schema` 输出为准。执行操作使用显式的 `command run`，不混入数据 API。

## 输出和退出码

成功：

```json
{"schemaVersion":1,"ok":true,"data":{"id":"COMMAND_ID","revision":4}}
```

失败：

```json
{"schemaVersion":1,"ok":false,"error":{"code":"REVISION_CONFLICT","message":"Data changed. Fetch the latest object and retry your patch.","details":{"expectedRevision":3,"actualRevision":4}}}
```

`list` 的 `data` 为 `{"items":[...],"total":N}`。列表默认按稳定 ID 排序，可用 `--query` 搜索名称、描述和命令内容（项目列表还搜索目录）。项目列表默认包括归档项目，用 `--archived false` 仅查活动项目。

| 退出码 | 含义 |
| --- | --- |
| 0 | 操作成功 / 执行命令返回 0 |
| 1 | 数据库、文件或终端等运行错误 |
| 2 | 参数、JSON、数据校验错误 |
| 3 | 项目或命令不存在 |
| 4 | 重复 ID/目录，或版本冲突 |
| 10 | 已执行命令返回非零退出码 |
| 124 | 已执行命令超时 |

`command run` 的退出码 10/124 仍返回 `data`（包含输出与执行结果），同时 `ok:false`，而不是丢弃结果变成纯错误。其他错误走 `error` 对象。普通操作的 stdout 仅有一个 JSON 对象，stderr 不输出进度或日志；`--help` / `--version` 为可读文本。

## 并发与数据位置

桌面与 CLI 直接调用同一个核心，SQLite 使用 WAL、5 秒忙等待和事务写入。命令的修改只更新目标命令，其他命令会保留；失败的校验或版本检查不会写入部分数据。

`--if-revision` 可选，但 AI 更新/删除时建议始终提供：发现冲突后重新 `get`，根据最新对象重新组织 patch。项目和命令各有 revision；命令变化也会提高所属项目 revision。打开项目会更新最近打开时间及项目 revision。

数据目录优先级：CLI 的 `--data-dir` > `RUNBOOK_DATA_DIR` > 系统本地应用数据目录下的 `Runbook`。`runbook info` 会报告实际数据库位置。桌面使用同样的环境变量和默认目录。

# AI 使用 Runbook

使用 `dist/runbook.exe` 管理命令，不需要打开窗口或直接编辑 SQLite 文件。

可复用的 Agent Skill 见 [runbook-cli](skills/runbook-cli/SKILL.md)，包含程序定位、快捷添加项目、版本冲突处理和执行结果解析。

1. 先执行 `dist/runbook.exe info` 确认数据目录。用 `schema --list` 查看 JSON API 操作名，再用 `schema --op command_create` 等按需读取规则；无需每次读取完整 Schema。无筛选的 `schema` 仍提供完整协议，所有 Schema 查询都不访问数据库。
2. 用 `project list` 找到项目 ID，再用 `command list --project ID` 找到命令 ID。不要用数组位置或名称作为修改目标。
3. 创建对象使用 `--input FILE`，或 `--input -` 从 stdin 输入 JSON。文件编码为 UTF-8。
4. 修改前 `get`，取得 `revision` 后使用 `--if-revision N`；只提交需要修改的字段。
5. `params` 是完整替换：先读取现有参数，保留未修改的参数再提交。
6. `REVISION_CONFLICT` 表示其他 GUI/CLI 写入了新版本，重新读取并合并意图，不要盲目丢弃版本检查。
7. 普通结果与错误都是 stdout 的单个 JSON 对象；检查进程退出码及 `ok`，不要仅看有没有输出。`--help` / `--version` 例外，为人类可读文本。
8. 增删改查和 `render` 不会执行 Shell 命令。只有用户要求执行时才调用 `command run`，也不要把 AI 的系统指令放进 `base`。

CLI 支持：

```text
runbook info
runbook gui [PATH]
runbook schema
runbook schema --list
runbook schema --op OP
runbook project list [--query TEXT] [--archived true|false]
runbook project get --id ID
runbook project add [PATH] [--name NAME] [--description TEXT]
runbook project create --input FILE_OR_STDIN
runbook project update --id ID --if-revision N --input FILE_OR_STDIN
runbook project delete --id ID --if-revision N
runbook command list --project ID [--query TEXT]
runbook command get --project ID --id ID
runbook command create --project ID --input FILE_OR_STDIN
runbook command update --project ID --id ID --if-revision N --input FILE_OR_STDIN
runbook command delete --project ID --id ID --if-revision N
runbook command render --project ID --id ID [--values FILE_OR_STDIN]
runbook command run --project ID --id ID [--values FILE_OR_STDIN] [--if-revision N] [--timeout-ms N]
runbook api --input FILE_OR_STDIN
```

全部命令支持全局 `--data-dir DIR` 和 `--pretty`。完整协议和示例见 `docs/CLI.md`。

用户要求打开桌面时可用 `runbook gui [PATH]`，默认当前目录。复用包含该目录的最近项目（已归档则恢复），未匹配时注册项目并打开；目录必须存在。结果位于 `data.project`，`data.created` 表示是否创建，`data.pid` 是新桌面进程。其他数据管理任务不需要调用 `gui`。

默认数据在可执行文件旁的 `data/runbook.db`，不再使用 C 盘 AppData；显式 `--data-dir` 和 `RUNBOOK_DATA_DIR` 仍有效。首次数据操作会初始化应用目录对应的 Runbook 默认项目，仅含两个日常命令；查找用户业务项目时不要将它误当作当前工作目录，也不要清空它来制作分发包。发布包排除整个本机 `data`，由程序首次运行生成默认内容。

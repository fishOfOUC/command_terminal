# AI 使用 Runbook

使用 `dist/runbook.exe` 管理命令，不需要打开窗口或直接编辑 SQLite 文件。

可复用的 Agent Skill 见 [runbook-cli](skills/runbook-cli/SKILL.md)，包含程序定位、快捷添加项目、版本冲突处理和执行结果解析。

1. 先执行 `dist/runbook.exe info` 确认数据目录，执行 `dist/runbook.exe schema` 获取实际 JSON Schema。
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
runbook schema
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

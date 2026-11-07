# 便携版更新

项目页和命令工作台右上角都有“检查更新”。应用每天最多自动检查一次，手动检查不受限制。检查和下载在后台进行，可以关闭对话框继续工作。下载完成后选择“重启并更新”，确认结束终端会话，应用会退出、替换程序并重新打开原项目。已完成的下载在下次启动时仍可使用。

更新同时替换 `Runbook-Desktop.exe`、`runbook.exe`、`runbook-updater.exe` 和随包文档。程序目录与文件名保持不变，已注册的全局 CLI 无需重新注册。`data`、`RUNBOOK_DATA_DIR` 指定的数据目录、主题、WebView 缓存和其他用户文件不在替换范围内。

## 首次升级

原来的 v0.1.0 不包含更新功能，需要退出全部 Runbook 窗口和 CLI，将 v0.1.1 便携包解压覆盖到原目录一次，保留原来的 `data`。之后的正式版本可通过应用内更新。

## 发布新版本

更新清单地址固定为：

```text
https://github.com/fishOfOUC/command_terminal/releases/latest/download/latest.json
```

1. 同步修改 `Cargo.toml` 的 workspace version、`package.json`、`package-lock.json` 的根版本和 packages 根版本、`src-tauri/tauri.conf.json` 的 version。使用正式 SemVer，例如 `0.1.2`。
2. 更新 `docs/RELEASE_NOTES.md`，运行 `node scripts/check-version.mjs`。
3. 执行测试、构建和签名：

```powershell
cargo test --locked -p runbook-core -p runbook-cli -p runbook-update
.\build.ps1 -SignUpdate -ReleaseNotes docs/RELEASE_NOTES.md
```

4. 在对应 `v版本号` 的 GitHub Release 中上传 `release/Runbook-版本号-windows-x64.zip` 和 `release/latest.json`。两份文件都上传完整、验收通过后，再发布为正式 Latest Release。

也可使用 `.github/workflows/release.yml`：先配置仓库 Actions Secret `RUNBOOK_SIGNING_KEY_PEM`，内容为现有私钥 PEM。推送与源码版本一致的 `v*` 标签后，工作流测试、构建、签名并创建草稿 Release，由维护者检查后发布。工作流不会自动发布草稿。

## 签名密钥

客户端内置 `assets/update-public-key.txt`，使用 Ed25519 验证整个清单。清单包括版本、平台、数据结构版本、ZIP 地址、长度和 SHA-256。

本次初始化的私钥在本机 `.keys/update-private.pem`，已通过 `.gitignore` 排除。请安全备份，不放入仓库、发布包或日志。其他电脑可用环境变量 `RUNBOOK_SIGNING_KEY` 指定私钥文件路径，或用 `RUNBOOK_SIGNING_KEY_PEM` 提供内容。签名脚本检查私钥是否匹配内置公钥。

`node scripts/sign-update.mjs --init` 仅用于尚无公钥和私钥的新项目，已有任一密钥时拒绝覆盖。丢失私钥后，需要让用户手动安装信任新公钥的版本；直接修改仓库公钥无法更新现有客户端。

## 失败与恢复

- 网络失败、包不完整、签名错误或哈希错误：保持当前应用，显示错误，可重试。
- 发布源尚无 `latest.json`：明确显示“发布源尚未提供更新清单”，不会误报为已是最新版。
- 其他 Runbook 窗口或 CLI 未退出：更新器最多等待 60 秒，不强杀其他实例。关闭它们后重新启动应用并重试。
- 文件替换失败或新版未在 45 秒内完成启动确认：恢复程序备份，尝试重新打开旧版。
- 替换时断电或更新器异常退出：下次启动读取事务日志，退出当前进程，由独立更新器恢复旧文件并重新打开，避免覆盖正在运行的程序。

更新结果、缓存和备份在程序旁的 `.runbook-update`，完成后的残留文件在后续启动时清理。上次结果在更新对话框显示。协议目前只接受数据库结构版本 1，不执行数据库迁移。

## 验证

`cargo test -p runbook-update` 检查签名、版本比较、包损坏、非法路径、文件替换、并发实例和中断恢复，现有核心及 CLI 回归也必须通过。

维护者可运行 `node scripts/update-smoke.mjs`，用本机私钥为隔离测试包签名，在真实 WebView2 桌面验证有效更新、缺失清单、签名篡改、包损坏和坏版本回滚。需要 `.qa/update-old` 下的旧版更新功能程序和 `target/debug` 下的新版程序；旧版必须低于当前版本。测试不访问日常 `dist/data`。

仅 Debug 构建允许 `RUNBOOK_UPDATE_ENDPOINT` 覆盖更新源，并允许 localhost/127.0.0.1 的 HTTP 测试源。Release 忽略此覆盖；清单和所有下载重定向都要求 HTTPS。两种构建均要求内置公钥的签名。

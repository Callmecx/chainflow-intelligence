# Windows 运维与故障诊断

## 日常运行

在项目根目录运行 `scripts/windows/start.ps1`。首次构建下载公开开源依赖，需要互联网和足够磁盘；随后可以加 `-SkipBuild` 使用已编译产物。脚本自动在当前进程初始化已安装的 MSVC 工具环境，不更改全局系统配置。所有端口仅监听 `127.0.0.1`。

本机要持续监控，必须保持电脑运行、网络可用、Rust 进程存活。休眠后会出现采样空窗，恢复时不得把空窗补成真实历史。扫描默认间隔 120 秒，另受来源限流影响。

日志为 `logs/backend-时间.log`、`logs/backend-时间.error.log`、`logs/frontend-时间.log` 和 `logs/frontend-时间.error.log`。结构化后端日志可能写在标准输出或错误流，两者都应检查。每次启动生成新日志；日志轮换按启动分段，本版未实现运行中的按日自动压缩清理。需要时在停机后手动归档旧日志。

## 停止与重新启动

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\stop.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\start.ps1 -SkipBuild
```

停止脚本校验工作目录、PID、启动时间、可执行文件和命令行项目路径；不匹配则拒绝停止。不得使用 `taskkill /IM node.exe /F` 等全局批量停止命令。SQLite 使用 WAL，应保留 `.sqlite-wal` / `.sqlite-shm` 文件；异常停机后重启时由 SQLite 恢复。

## 排查顺序

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\doctor.ps1
Invoke-RestMethod http://127.0.0.1:8787/api/health
Invoke-RestMethod http://127.0.0.1:8787/api/sources
Invoke-RestMethod http://127.0.0.1:8787/api/metrics
```

| 现象 | 检查 / 处理 |
| --- | --- |
| 找不到 cargo / rustc | 安装 Rust，或把便携 bin 绝对路径写入 `.runtime/toolchain-path.txt`；重新运行诊断。 |
| link.exe / C 编译器缺失 | 安装 VS C++ Build Tools 和 Windows SDK；在 x64 Native Tools 终端运行构建。 |
| npm 执行策略阻止 | 脚本使用 `npm.cmd`，无需修改系统执行策略；PowerShell 脚本可用单次 `-ExecutionPolicy Bypass`。 |
| 8787 / 5178 被占用 | doctor 输出监听 PID；先核对所属项目，不自动杀占用进程。前端可用 `start.ps1 -FrontendPort 5179`。 |
| API 正常但没有行情 | 等待首次真实扫描，查看 sources 的状态/错误；health 只说明进程/API 可用。 |
| 429 / 503 | 尊重 Retry-After / 退避，降低调用量；不要复制多个进程绕过额度。 |
| 403 / 网络拒绝 | 记录来源受限状态；不要绕过访问控制。检查正常网络和上游服务状态。 |
| 图表为空或结果 UNKNOWN | 尚无足够采样；公共快照没有连续数据库。保留空状态，不填样例。 |
| Pages 旧数据 | 看实际 UTC 时间和 STALE；检查 Actions 定时是否停用、作业失败或排队。 |
| SQLite 锁或损坏疑虑 | 先停机备份；确认只有一个本项目服务；运行 CLI doctor 检查完整性。不要删除原数据库。 |

## Canary 运行记录

先启动本地服务，再在另一个 PowerShell 终端执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\canary.ps1 -DurationMinutes 15
```

脚本只请求已运行的 `127.0.0.1:8787`，不启动、停止或重启任何服务。默认每 60 秒记录健康状态、样本/信号数、来源请求/错误计数及最近成功延迟，并通过本机进程的真实 WorkingSet64 记录内存。等待至少 15 分钟后输出 `docs/evidence/canary.json`。证据不包含 PID、个人地址清单、本机路径、凭证或原始错误正文。

文件在每次采样后写入，运行中标记 RUNNING；中断留下的文件不能算完成。结束后的 COMPLETED 只表示观察时长完成，来源失败、接口错误和数据库完整性另有独立字段，不会一律标为 PASS。跨重启的计数差值不可比较，脚本会将相应增量记为未知。不同次运行会更新该证据文件，公开前应核对实际 UTC 起止时间。

## 备份 SQLite

最简单可靠的方法是停止本项目服务后，复制数据库及当时存在的 WAL/SHM 为同一备份集合。不要只复制正在写入的主 `.sqlite` 文件。下例只复制明确命名的项目数据文件，不会删除原文件：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\stop.ps1
$backupDir = Join-Path (Get-Location) ('data\backups\' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
foreach ($name in @('chainflow.sqlite', 'chainflow.sqlite-wal', 'chainflow.sqlite-shm')) {
    $source = Join-Path 'data' $name
    if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination $backupDir }
}
```

备份包含个人观察名单和研究历史，必须留在本机或用户自己控制的安全位置，不加入 Git。恢复前先停止服务并另存现有数据库；把同一备份集合恢复至独立目录验证，通过 doctor 后再决定替换。不要混用不同时间的主库与 WAL。

## 归档与离线恢复

```powershell
cargo run --locked --manifest-path backend/Cargo.toml -- archive --db data/chainflow.sqlite --output data/archive.json --retention-days 30
cargo run --locked --manifest-path backend/Cargo.toml -- restore --db data/restored.sqlite --input data/archive.json
cargo run --locked --manifest-path backend/Cargo.toml -- doctor --db data/restored.sqlite
```

先停机再执行维护命令；用新的 `restored.sqlite` 做恢复验证。`archive` 将超过保留期的 samples 写入新 JSON 文件，同步成功后才删除对应数据库样本；拒绝覆盖已有归档。`restore` 幂等恢复样本行，不等同于完整恢复观察名单、信号和最新视图；完整恢复应使用 SQLite 备份集合。归档含历史数据，不应公开。

本机 `serve` 每完成 720 轮扫描自动归档 30 天以前的采样至数据库邻接的 `archives` 目录。默认 120 秒间隔下约为一天，进程重启后轮数重新计数；它不是操作系统定时任务，也没有运行中按日日志轮转。自动归档真实运行证据应在长期验收中另行记录，不能由短 Canary 推断通过。

## 凭证处理

本版本未接入 Telegram，不需要 Bot Token、Chat ID 或 GitHub Secrets。`.env.example` 只列实际支持的本地日志和规则配置。若此前在其他系统使用的 Telegram Bot Token 泄露，在 Telegram 官方 BotFather 撤销旧 token 并生成新 token，更新真正使用它的本地配置/Secrets，再重启对应服务；删除旧日志或 Git 历史中的凭证不能代替撤销。ChainFlow 当前不读取、不发送、不验证这些凭证。

GitHub 登录由 `gh auth login` 管理；需要退出时用 `gh auth logout`。不要将 `gh auth token` 输出复制进日志、Issue、页面或本项目。任何钱包私钥都与本项目无关。

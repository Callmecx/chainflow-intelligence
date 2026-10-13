# Windows 运维与故障诊断

本文适用于 V1.1。求职作品封版范围为 PORTFOLIO_READY；Telegram 真实投递和 6h/24h 自然长跑仍为 PENDING，不作为本次发布硬门槛。

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
| 429 / 5xx | 采集 HTTP 对 429 与全部 5xx 尊重 Retry-After / 有限退避；降低调用量，不复制多个进程绕过额度。 |
| 403 / 网络拒绝 | 记录来源受限状态；不要绕过访问控制。检查正常网络和上游服务状态。 |
| 图表为空或结果 UNKNOWN | 尚无足够采样；公共快照没有连续数据库。保留空状态，不填样例。 |
| Pages 旧数据 | 看实际 UTC 时间和 STALE；检查 Actions 定时是否停用、作业失败或排队。 |
| SQLite 锁或损坏疑虑 | 先停机备份；确认只有一个本项目服务；运行 CLI doctor 检查完整性。不要删除原数据库。 |
| Telegram 未发送 | 先看 health 中 configured / enabled / verification 与队列状态；缺凭据、未启用、暂无合格新事件都不能当作已经送达。详见 TELEGRAM_SETUP.md。 |
| API 为 OK，但观察器报 UNAVAILABLE | 核对观察脚本版本和 error_kind；V1.1 已修复空通知队列 `{}` 触发 PowerShell StrictMode 的误报，不能把该脚本错误计成 Provider 失联。 |

## Canary 运行记录

先启动本地服务，再在另一个 PowerShell 终端执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\run-canary.ps1 -DurationMinutes 15
```

脚本只请求已运行的 `127.0.0.1:8787`，不启动、停止或重启采集服务。默认每 60 秒记录健康、样本、WATCH / RISK / CONFIRMATION / DATA_FAULT 分类计数、通知队列、来源请求/错误/最近成功时间、进程 CPU 与 WorkingSet64、数据缺口和数据库完整性。真实采样至少 15 分钟后，结果位于 `.runtime/soaks/<Run ID>/report.json` 和 `report.md`。

每个 Run ID 保留独立记录，运行中标记 RUNNING；中断不能算完成。COMPLETED 只表示观察时长完成，错误、缺口和数据库完整性另有字段。跨重启或缺少基线的计数不会推算成整个窗口的完整增量。`soak-stop.ps1` 只停止身份匹配的观察器，不停止 Rust/前端；`soak-report.ps1 -RunId <实际ID> -ExportDirectory <目录>` 导出过滤后的 JSON/Markdown，已有目标拒绝覆盖。公开报告不含 PID、本机路径、个人地址清单、密钥或原始错误正文。

6h/24h 独立后台运行、状态、恢复和导出命令见 [SOAK_RUN_GUIDE.md](SOAK_RUN_GUIDE.md)。旧 `canary.ps1` 和 `docs/evidence/canary.json` 是 V1 历史证据，V1.1 使用新脚本，避免覆盖旧验收。已保留的观察器误报与修复证据见 [v1-1-canary-observer-fix.json](evidence/v1-1-canary-observer-fix.json)；后端当时继续采集，失败记录不算自然 Canary 通过。

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

V1.1 首次打开旧 schema 时会先执行 SQLite `VACUUM INTO`，在数据目录旁 `backups/` 创建迁移备份并校验 integrity，再增加 schema v2 的表。`doctor` 也通过 `Store::open` 打开数据库，因此对旧 schema 运行新版 doctor 可能触发迁移；升级前应先执行上述停机备份，不能把 CLI doctor 当成任意旧库上的绝对只读检查。更高版本数据库会被拒绝降级打开。迁移实测记录见 [v1-1-migration-runtime.json](evidence/v1-1-migration-runtime.json)。

## 单次公开扫描

```powershell
cargo run --locked --manifest-path backend/Cargo.toml -- scan-once --output frontend/public/snapshot.json
```

省略 `--db` 会创建唯一临时数据库；指定 `--db` 时必须是尚不存在的新文件。已有数据库会被拒绝，不能传 `data/chainflow.sqlite` 绕过本地/公开边界。单次扫描不继承个人关注历史，不运行通知投递任务；输出仅用于 PUBLIC_SNAPSHOT。离线 `--fixture` 明确生成 DEMO_FIXTURE，不能用于真实 Pages 数据验收。

## 归档与离线恢复

```powershell
cargo run --locked --manifest-path backend/Cargo.toml -- archive --db data/chainflow.sqlite --output data/archive.json --retention-days 30
cargo run --locked --manifest-path backend/Cargo.toml -- restore --db data/restored.sqlite --input data/archive.json
cargo run --locked --manifest-path backend/Cargo.toml -- doctor --db data/restored.sqlite
```

先停机再执行维护命令；用新的 `restored.sqlite` 做恢复验证。`archive` 将超过保留期的 samples 写入新 JSON 文件，同步成功后才删除对应数据库样本；拒绝覆盖已有归档。`restore` 幂等恢复样本行，不等同于完整恢复观察名单、信号和最新视图；完整恢复应使用 SQLite 备份集合。归档含历史数据，不应公开。

本机 `serve` 每完成 720 轮扫描自动归档 30 天以前的采样至数据库邻接的 `archives` 目录。默认 120 秒间隔下约为一天，进程重启后轮数重新计数；它不是操作系统定时任务，也没有运行中按日日志轮转。自动归档真实运行证据应在长期验收中另行记录，不能由短 Canary 推断通过。

## 凭证处理

V1.1 支持本机可选 Telegram：`.env.example` 列出 `TELEGRAM_ENABLED=false`、`TELEGRAM_BOT_TOKEN` 和 `TELEGRAM_CHAT_ID`。仅在用户自己的编辑器中填写；Windows start 脚本把白名单配置载入子进程，直接 `cargo run` 不会自动读取 `.env`。默认关闭、缺凭据或配置无效时行情监控继续运行。设置与投递状态说明见 [TELEGRAM_SETUP.md](TELEGRAM_SETUP.md)。

当前真实 Telegram 投递 **PENDING**；mock、队列入库或 health 可用均不等于聊天收到消息。网络结果无法确认时保留 UNCERTAIN，不自动重发。若 Bot Token 泄露，应在 Telegram 官方 BotFather 撤销旧 token、生成新 token，只更新本机 `.env` 后重启；仅删除日志或 Git 历史不能代替撤销。公开 Pages 和 Actions 无需 Telegram Secrets，不要把这些凭据放入公开快照、日志、截图或验收报告。数据库中的通知正文也属于本地数据。

GitHub 登录由 `gh auth login` 管理；需要退出时用 `gh auth logout`。不要将 `gh auth token` 输出复制进日志、Issue、页面或本项目。任何钱包私钥都与本项目无关。

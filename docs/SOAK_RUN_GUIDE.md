# Windows 短时与长跑观察

这些脚本在本机只读访问 `127.0.0.1:8787`，记录真实后端状态；不执行交易，不启动新采集实例，不操作数据库迁移。Rust 服务与观察进程各自独立，关闭 Codex 不会结束后台长跑。关机或退出 Windows 会中断本机进程，脚本不会把中断时间补成已验证监控。

## 先启动现有服务

在 PowerShell 中进入项目：

```powershell
Set-Location D:\ChainFlow-Intelligence
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\doctor.ps1
# 未运行时启动；已运行不要再启第二份。
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\start.ps1 -SkipBuild
```

看板默认 `http://127.0.0.1:5178/chainflow-intelligence/`。确认来源状态后开始观察。`.env` 只在启动服务时加载，Telegram 密钥不会进入长跑记录；修改 Telegram 配置后需由用户按运维说明重启服务。

## 15–30 分钟自然 Canary

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\run-canary.ps1 -DurationMinutes 15
# 或 -DurationMinutes 30
```

这是前台观察命令，每分钟真实请求 health 与 tokens、读取本项目进程 CPU / 内存、核对数据时间与 SQLite 完整性。等待只用于控制实际采样频率。结束后打印本地报告位置；Ctrl+C 或进程异常结束不能算完成。V1 的 `canary.ps1` 保留用于旧证据复现，V1.1 使用 `run-canary.ps1`。

## 用户手动启动 6 / 24 小时

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\run-soak.ps1 -Hours 6
# 或 -Hours 24
```

命令会隐藏窗口启动独立 PowerShell 观察进程，返回一个 Run ID。可关闭当前终端和 Codex；Rust 服务需继续运行。此脚本不会修改电源计划、创建付费云机器或配置系统自动登录。请用户自行保持电脑运行和网络可用。

所有运行资料放在 `.runtime/soaks/<Run ID>/`，被 Git 忽略：

| 文件 | 内容 |
| --- | --- |
| state.json | 本地工作目录、观察进程身份、目标时长和心跳；不公开 |
| observations.jsonl | 每次真实观测的聚合数据，不含 token 地址清单和密钥 |
| observer-UTC时间.log / observer-UTC时间.error.log | 每次启动的后台观察日志；恢复时保留旧日志；后端继续使用独立日志 |
| report.json / report.md | 自动生成的可审阅报告；RUNNING 不代表完成 |
| stop.request | 请求观察进程停止的标记 |

Run ID 不包含账户信息。没有指定 `-RunId` 时，管理命令使用最近创建的观察任务；同时运行多个任务时请总是指定 Run ID，避免对错任务操作。

## 状态、停止与恢复

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\soak-status.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\soak-stop.ps1 -RunId 这里替换为实际RunID
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\soak-report.ps1 -RunId 这里替换为实际RunID
```

`soak-stop` 仅请求该任务的观察进程退出，**不会停止 Rust 或前端**。停止前校验 PID、启动时间、可执行文件和本项目脚本路径；身份不匹配不杀进程。网络请求未结束时可稍后检查；只有确需强制结束这个已核验观察进程时才用 `-Force`。停止采集服务仍用原来的 `stop.ps1`，请先确认没有正在进行的验收。

若观察进程被意外关闭，或 Windows 重启/恢复后需要继续同一份记录：

```powershell
# 先恢复现有Rust服务，再恢复未完成的观察记录。
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\run-soak.ps1 -Resume 这里替换为实际RunID
```

恢复保留已有观察，不清空数据，增加 resume_count，并把中间没有观测的时间记为缺口。已经完成的报告不能恢复成新的任务，需重新创建运行。检测到观察进程失踪时，`soak-status` / `soak-report` 将 RUNNING 记录报告为 INTERRUPTED，不把旧心跳误报为服务还活着。

## 报告口径

- **实际跨度与有效覆盖分开**：实际跨度来自第一/最后观测的 UTC；有连接的观测间隔只累计相邻成功且未超过容差的区间。跨度达到 6h 不代表其中每分钟都成功监控。
- **缺口**：相邻观测超过 150 秒或时钟倒退标记 HOST_OR_OBSERVER_GAP_OR_CLOCK_CHANGE，可能来自休眠、断电、进程挂起等，不能仅凭采样断定具体原因。行情历史相邻样本超过 300 秒/不递增、单池数据陈旧均单列计数。
- **恢复**：API 失败后重新成功、Provider SOURCE_ERROR / UNAVAILABLE / STALE 后回到 OK，分别计数；后端运行起点改变计重启转换。没有发生故障时恢复数为 0，不写成“恢复验证通过”。
- **计数**：Provider 请求/失败、采样增量仅累加同一运行实例的相邻可比较观察；重启和缺失基线不会当成 0。`counter_coverage_complete=false` 表示增量不是整个墙钟窗口完整计数。
- **分类信号**：WATCH、RISK、CONFIRMATION、DATA_FAULT 分开，来自 SQLite 全表聚合。旧版 API 缺少分类时显示 UNKNOWN；绝不把最后 200 条信号列表当全量，也不把 WATCH 全算风险。
- **通知**：记录队列各投递状态数量，不包含 Bot Token、Chat ID、正文或代币地址。没有真实 Telegram 发送证据时不宣称通知已送达。分类与通知字段缺失时为 UNKNOWN。
- **CPU / 内存**：读取已核验为本项目 Rust 二进制的 `Get-Process` 实际 CPU 累计秒与 WorkingSet64；CPU 占比按时间差及全机逻辑核数计算。不能读取时为 UNKNOWN，不使用估计常数。
- **数据库**：health 接口返回的 integrity_check / WAL 状态进入报告。这里只检查，不复制或修改数据库；完整备份和迁移由维护流程单独处理。

COMPLETED 只表示计划观察结束，不代表免费 Provider 永久可靠、信号有效或策略获利。必须同时看错误、缺口、请求、样本及数据库字段。

## 导出与公开证据

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\soak-report.ps1 -RunId 这里替换为实际RunID -ExportDirectory .\docs\evidence\v1-1-soak
```

只复制已过滤的 Markdown / JSON 报告，文件名包含 Run ID；已存在时拒绝覆盖。先审阅再决定提交。不要提交 `.runtime`、原始后台日志、数据库、个人观察名单或 `.env`。

## 本轮验收状态

6 小时真实长跑：**PENDING**。24 小时真实长跑：**PENDING**。脚本的离线夹具测试和十秒辅助采样只能证明对应代码路径，不能替代自然 6/24 小时运行；短 Canary 的结果由 V1.1 实际验收记录另行填写。

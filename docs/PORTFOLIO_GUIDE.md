# ChainFlow Intelligence 求职展示指南

这是一个可运行、可读源码、可复查证据的 Rust 数据工程与全栈作品：使用免费公开来源观察 Solana / BSC 池行情，本地持续采集，GitHub Pages 展示定时快照。它适合展示异步 I/O、限流重试、数据建模、SQLite 持久化、可解释规则和测试边界。

- 源码：[Callmecx/chainflow-intelligence](https://github.com/Callmecx/chainflow-intelligence)
- 公开体验：[GitHub Pages](https://callmecx.github.io/chainflow-intelligence/)
- 真实验收证据：[V1_1_ACCEPTANCE.md](V1_1_ACCEPTANCE.md)、下文列出的 [evidence/](evidence/) 和 V1 历史 [ACCEPTANCE.md](ACCEPTANCE.md)。提交版本和验证时间需要一起阅读；旧版本测试不会自动证明新版本已通过。

## 三分钟演示

演示前按 [SOAK_RUN_GUIDE.md](SOAK_RUN_GUIDE.md) 检查本地服务。没有新鲜数据时直接演示实际错误/陈旧状态；不要用离线 fixture 替换后再称其为实时数据。

| 时间 | 操作 | 建议讲述 |
| --- | --- | --- |
| 0:00–0:30 | 打开公开 Pages，指向运行模式、生成时间和来源 | “这个网址是 GitHub Actions 生成的公开快照。本地 Rust 进程才提供持续监控；浏览器刷新不等于重新采集链上数据。” |
| 0:30–1:10 | 打开本地看板，按 SOL/BSC 筛选，选一个有真实数据的池 | “身份由链、完整代币地址和池地址组成。同名币不合并；不同池独立保存。MarketCap、FDV 和未知值分别展示。” |
| 1:10–1:45 | 查看详情的样本图、来源时间、结构解释和事件证据 | “这些点是带来源时间的 SAMPLED 采样。确认有持续时间与连续性要求，价格缺失、过期或来源切换时拒绝推断。关注、确认、市场风险、数据故障分别计数。” |
| 1:45–2:15 | 查看来源健康与本地健康面板 | “Provider 失败会留下状态。来源限流、请求失败和数据新鲜度可追踪；本地 SQLite 状态及通知队列可见。没有 Bot 配置时如实显示未验证。” |
| 2:15–2:40 | 打开信号研究 | “记录正式信号发生时的价格、规则版本和证据，再观察 1h/2h/6h。未到时间是 PENDING，缺行情是 MISSING 或 CENSORED，样本不足不做有效性结论。” |
| 2:40–3:00 | 打开一次真实验收 JSON 和对应源码 | “测试夹具、真实短时联网记录和长跑记录分开。可以看到实际 UTC 起止、观察次数、错误与数据库完整性；没有测过的时间长度不会写成通过。” |

公开模式不展示本机个人关注数据库或 Telegram 密钥。演示无需钱包连接、私钥或交易签名。

## Rust 源码阅读路线

| 文件 | 能核对的工程内容 |
| --- | --- |
| [main.rs](../backend/src/main.rs) | `serve` / `scan-once` / `doctor` / 归档恢复 CLI，Tokio 定时采集、独立通知任务、回环监听与 Ctrl+C 退出 |
| [providers/mod.rs](../backend/src/providers/mod.rs) | `Provider` 接口、`ProviderHub` 并行调用、免费来源预算、精确地址查询、显式 FixtureProvider |
| [providers/http.rs](../backend/src/providers/http.rs) | reqwest 连接/请求超时，按来源类别共享的限流门，Retry-After、退避、TTL 缓存、健康计数 |
| [providers/parsing.rs](../backend/src/providers/parsing.rs) | 原始 JSON 到可空标准字段的转换、来源标记、地址与池去重 |
| [models.rs](../backend/src/models.rs) | 池身份、快照、采样点、信号和来源状态；地址规范化与 UTC 时间 |
| [api.rs](../backend/src/api.rs) | axum API、扫描互斥、运行计数、个人关注及写请求来源限制 |
| [storage.rs](../backend/src/storage.rs) | 单 SQLite 工作线程、64 容量队列、WAL、事务、幂等入库、迁移前备份和完整性检查 |
| [engine.rs](../backend/src/engine.rs) | 生命周期、候选确认、风险和恢复条件；保留规则证据 |
| [structures.rs](../backend/src/structures.rs) | 3m/5m/15m 的采样覆盖约束、同源性、结构分项和规则版本 |
| [research.rs](../backend/src/research.rs) | 不变信号锚点、前瞻窗口、终点容差、缺失/删失分母和描述统计 |
| [notifications.rs](../backend/src/notifications.rs) | SQLite 持久队列、冷却/过期/重试、崩溃后不确定状态、凭据脱敏 |
| [config.rs](../backend/src/config.rs) | 可配置阈值与输入校验；默认值由源码直接确定 |

前端入口为 [App.vue](../frontend/src/App.vue)，数据边界在 [api.ts](../frontend/src/api.ts)，信号解释、研究、健康和图表分别位于 [components/](../frontend/src/components/)。前端明确核对 LOCAL_LIVE / PUBLIC_SNAPSHOT，不接受伪装成线上数据的 DEMO_FIXTURE。

## Tokio 具体做了什么

`ProviderHub::scan` 使用 `tokio::join!` 同时等待 DEX Screener、GeckoTerminal 与 RPC 三组 I/O。每组仍遵守请求预算和共享限流。这里没有把“用了异步”包装成未经测试的高吞吐性能；实际扫描用时受免费接口和主动限流影响。

`serve` 为周期扫描创建独立 Tokio 任务，使用 `interval` 和 `MissedTickBehavior::Delay`。扫描互斥锁避免并发采集冲突；一轮返回错误会写入日志与运行错误计数，下一轮仍可运行。通知开启后，另一个任务每 5 秒尝试领取一条队列消息，不把网络发送放进 SQLite 写事务。

rusqlite 操作在专用系统线程内执行。异步调用用有界 `sync_channel` 的 `try_send` 提交工作，再用 Tokio `oneshot` 等待结果；队列满时返回错误，避免在 Tokio 执行线程上等待同步队列空间。WAL 不等于无限并发，单写入模型是这个本地应用的明确取舍。

`Http` 使用 5 秒连接超时与 10 秒总请求超时，可重试失败最多尝试 3 次。429 与全部 5xx 尊重 Retry-After，过长冷却保留到后续请求；403 等非 429 的 4xx 不重试。缓存保留原 fetched_at，不能用缓存命中时间把旧数据刷新成“刚获取”。异常来源不会触发自动生成行情的回退路径。

## RPC 与行情源各自负责什么

| 来源 | 当前实际用途 | 不应据此宣称 |
| --- | --- | --- |
| DEX Screener | 资料/推广发现、按完整地址和池获取价格、流动性、成交量、买卖笔数等公开行情 | 免费接口覆盖所有新币；交易笔数等于净资金流 |
| GeckoTerminal | SOL/BSC 新池与趋势池发现、精确代币池查询，以及独立 OHLCV / 最近成交读取接口 | 每个池都有完整历史；应用采样图等于逐笔重建 K 线 |
| Solana 公开 RPC | `getAccountInfo` 读取基准 WSOL mint，验证节点/账户读取能力 | 所有候选代币都完成链上安全审计；已重建全链 Swap 流 |
| BSC 公开 RPC | `eth_chainId` / `eth_blockNumber` / WBNB `decimals` 的 `eth_call` 基础验证 | 公共端点支持持续全链日志索引；`latest` 就是最终确认 |

RPC 是当前系统的只读基础验证能力，USD 行情主要来自市场数据 API。`bsc_event_logs=UNSUPPORTED` 是明确的覆盖边界。蜜罐、持仓前十和聪明钱未经独立验证时保持 UNKNOWN。

## 面试时可展开的设计选择

- **数据真实性**：`None` 与 0 分开，MarketCap 与 FDV 分开；相同 symbol 不是身份。乱序、重复、缺价和跨来源样本不能用来伪造持续趋势。
- **状态解释**：WATCH 表示观察事件，RISK 表示风险，CONFIRMATION 需要满足持续条件；数据源异常用 DATA_FAULT。每条正式信号保留规则版本、触发时间和证据。
- **研究边界**：只在目标时点及容差到达后评估观察结果，缺口和来源切换明确删失；记录采样点之间的极值未知，不宣称收益优势。
- **通知可靠性**：投递持久化、限速、去重和冷却；网络发送后的状态写入或重启存在不确定窗口，用 UNCERTAIN 表示。Telegram 没有应用级幂等键，项目不承诺 exactly-once。
- **公开部署**：同一前端支持本机 API 和 Pages 静态快照。Actions 定时运行不承诺严格准点，公开站不是全天常驻采集服务器。
- **可验证运维**：Windows 脚本核对本项目进程身份再停止。长跑观察报告区分实际墙钟跨度、可观察连接区间和缺口，不把中断时间算成有效监控。

## 可直接使用的简历描述

**ChainFlow Intelligence｜Rust / Tokio / axum / SQLite / Vue 3 / TypeScript**

开发 Solana 与 BSC 公开行情监控作品，接入 DEX Screener、GeckoTerminal 和公开 RPC，以 Rust 异步采集、按源限流与退避、SQLite 单写入队列实现本地持续观察，并通过 GitHub Actions / Pages 发布免费定时快照。实现可解释状态机、分类风险事件、带证据的前瞻采样研究、持久化 Telegram 通知队列及中文数据看板；提供离线自动化测试、真实接口验证和 Windows 启停/诊断/运行观察报告。

简历中只填验收文件中已经记录的测试数量、时间长度和运行结果。不要添加未测量的 QPS、低延迟倍数、可用率、收益率、用户规模或“全链实时索引”等描述；Telegram 真实送达需要独立证据。

## 已有证据与可复核命令

V1 历史证据 [canary.json](evidence/canary.json) 记录 2026-10-13 08:19:27–08:34:28 UTC 的 16 次成功 API 观察、7 次完成扫描、1,012 条新增采样和最终 SQLite `ok`。其中 407 是所有分类事件的合计，不能称为 407 条风险预警。这份记录属于 V1，对应版本范围以 [ACCEPTANCE.md](ACCEPTANCE.md) 为准。

V1.1 Windows 观察脚本的 [离线测试](evidence/v1-1-windows-script-tests.json) 与 [真实辅助验证](evidence/v1-1-windows-auxiliary.json) 分开保存。后者只有 10.551 秒、3 次实际本地观察，用于证明观察器生命周期与安全停止，不能证明 15 分钟、6 小时或 24 小时监控稳定性；当时后端仍为 V1，缺少新增分类计数时报告为 UNKNOWN。

以下是目前已落盘的 V1.1 自动化结果，不是实时性能或市场有效性指标：

| 检查 | 实际结果 | 证据 |
| --- | --- | --- |
| Rust 后端 | 62 通过、0 失败 | [后端测试输出](evidence/v1-1-backend-tests.txt) |
| 前端单测 | 31 通过、0 失败 | [前端证据](evidence/frontend-v11-tests.json) |
| Playwright 桌面/手机 | 42 通过、0 失败；明确 fixture，包含故障快照回归 | [浏览器测试记录](evidence/frontend-v11-tests.json) |
| 前端 typecheck / lint / 生产 build | PASS | [前端证据](evidence/frontend-v11-tests.json) |
| Windows 观察脚本 | 28 通过、0 失败；明确 fixture | [PowerShell 测试](evidence/v1-1-windows-script-tests.json) |

真实 V1.1 本地浏览器记录另见 [v11-real-browser-smoke.json](evidence/frontend-v11-real-local/v11-real-browser-smoke.json)，真实隔离公开采集见 [v1-1-public-scan.json](evidence/v1-1-public-scan.json)。空通知计数曾导致观察器误报 UNAVAILABLE，已经修复并保留 [失败与回归记录](evidence/v1-1-canary-observer-fix.json)；不删除失败记录，也不把它算作自然 Canary 通过。最终 CI、自然 Canary 和公开 V1.1 部署结果以 [V1_1_ACCEPTANCE.md](V1_1_ACCEPTANCE.md) 为准。

```powershell
# 在项目根目录运行；Rust 与 Node 工具链需已可用。
cargo test --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings
npm --prefix frontend test
npm --prefix frontend run lint
npm --prefix frontend run build
npm --prefix frontend run test:e2e
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows/test-soak.ps1
```

这些命令需要实际执行成功才可以报告通过。离线测试数据会标明 fixture。V1.1 的最终测试数量、CI、真实 Canary 和发布 Commit 以最终验收记录为准。

目前 6h / 24h 自然长跑与 Telegram 真实投递证据为 **PENDING**。它们不作为本次 PORTFOLIO_READY 求职作品稳定封版的硬性门槛，也不据此宣称全天稳定性或消息已真实送达。现有作品可直接用于源码讲解、真实数据界面演示和工程设计讨论。

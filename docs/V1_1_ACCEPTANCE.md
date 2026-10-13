# V1.1 作品封版验收

核验日期：2026-10-13。状态：**PORTFOLIO_READY**。核心功能已冻结，自动测试、真实 Canary、远端 CI 与公开 Pages 实际浏览器验收通过。以下证据均保留实际版本和 UTC 时间；可选未完成项仍明确 PENDING。

本轮按最新作品定位验收：可真实运行的 Rust / Web3 工程、中文说明、真实接口、自动测试、短 Canary、公开源码及 Pages。Telegram 实投和 6h/24h 长跑不作为稳定作品版本的硬门槛，仍单独标为 PENDING。不会在本轮后自动开启下一轮功能开发。

## 版本与迁移

- V1 基线：`0c2260024dec39195ef8a771c1a4bf40847f0ed8`。
- 功能分支：`feature/v1.1-trader-monitoring`，保持原仓库历史，不强推。
- 核心后端：`d8ef177`；前端及 Windows 观察器：`90251ca`；空队列观察修复：`517052a`。
- Rust 服务于 `2026-10-13T09:43:20.324Z` 启动 V1.1，schema 升级至 2；迁移前自动一致性备份，历史保留，SQLite WAL / integrity `ok`。见 [迁移证据](evidence/v1-1-migration-runtime.json)。
- 最终发布 Commit、Tag、CI 与 Pages 运行见下方发布记录；历史运行证据不会冒称覆盖尚未产生的提交。

## 自动测试

| 项目 | 本地真实执行结果 | 覆盖及证据 |
|---|---|---|
| Rust all-targets | **62 passed / 0 failed** | 状态机、迁移、来源失败恢复、23 项通知 Mock、前瞻研究缺失/删失、提供器与 API；[原始输出](evidence/v1-1-backend-tests.txt) |
| Rust 格式与静态检查 | **PASS** | `cargo fmt --check`，`cargo clippy --locked --all-targets -- -D warnings` |
| Vue / TypeScript | **PASS** | typecheck、ESLint、生产构建；[前端报告](evidence/frontend-v11-tests.json) |
| 前端单测 | **31 passed / 0 failed** | 保留 20 项 V1 回归并新增研究、导出与隐私边界 |
| Playwright | **42 passed / 0 failed** | 桌面/手机 × 公开/本地，含 8 项 HTTP 200 故障快照及部分来源失败回归；独立 fixture，不算真实行情验收 |
| Windows 脚本 | **28 passed / 0 failed** | 错误恢复、缺口、计数 UNKNOWN、PID 身份校验、空通知队列；[明确离线 fixture](evidence/v1-1-windows-script-tests.json) |

本地真实浏览器于 `09:51:41–09:51:52 UTC` 通过：1124 个已保存池（不是同时新鲜的池数）、LOCAL_LIVE、健康 OK、SQLite ok，桌面与 412px 手机无横向溢出或浏览器错误，信号证据与白名单 JSON 导出可用。[报告及截图](evidence/frontend-v11-real-local/v11-real-browser-smoke.json)。

封版检查随后修复公开快照 HTTP 200 但来源失败时的首页提示（`ff807a7`）。全故障显示 SOURCE_ERROR / UNAVAILABLE，部分成功保留真实行情并警示，UNSUPPORTED 不冒称网络故障。修复后完整前端自动测试重新通过；原本地 smoke 时间保留，最终公开 smoke 覆盖发布后的代码。

## 真实接口与端到端案例

[Wrapped SOL 单池案例](REAL_TOKEN_CASE.md) 记录完整 mint / pool、5 次真实样本、来源时间、规则解释、RPC 与行情源的分工及旧版事件边界。独立 DexScreener 单池请求 HTTP 200，字段与当时本地详情一致。公开生成器使用全新隔离数据库，拒绝已有本机数据库，避免发布私人历史。

独立公开扫描于 `09:53:35–09:54:25 UTC` 真实执行，耗时 **50.58 秒**、退出码 0，获得 **68 池（SOL 41 / BSC 27）**。12 次请求中记录 2 次中间错误，Gecko BSC 新池请求重试后最终 OK；最终 `provider_errors=[]` 不能解释成全程零错误。现有本机数据库导出测试退出 1，未创建公开输出或临时输出；隐私字段检查通过。[完整白名单证据](evidence/v1-1-public-scan.json)。

正式信号研究在 `09:46:42 UTC` 记录数为 **0**，状态 `INSUFFICIENT_SAMPLE`。这不代表已完成 1h/2h/6h 自然窗口，也不代表收益为 0。升级前的确认事件不会回填成新规则样本。规则正确性由独立 fixture 验证，自然效果仍需实际新信号与后续时间。

## 15 分钟 Canary

完整新一轮：`20261013T094648Z-a1be38`，**COMPLETED / duration_met=true**。首个观察 `2026-10-13T09:46:49.4153217Z`，最后观察 `2026-10-13T10:01:54.1168489Z`，实际 **904.702 秒**。原始 [JSON](evidence/v1-1-canary/soak-20261013T094648Z-a1be38.json) / [Markdown](evidence/v1-1-canary/soak-20261013T094648Z-a1be38.md)。

| 实测项 | 结果 |
|---|---|
| API 观察 | **16 / 16 成功**，HTTP 观察错误 0，重启 0，观察器缺口 0 |
| 可比较计数 | 15 段，覆盖完整；新增 **1,758 条采样** |
| 来源请求 | 新增 **113** 请求、**1** 中间请求错误；最终可用来源 OK，BSC logs 仍 UNSUPPORTED |
| 信号增量 | WATCH 217、RISK 17、CONFIRMATION 2、DATA_FAULT 0；不把 WATCH 算作风险 |
| SQLite | integrity `ok`，WAL |
| Rust 工作集 | **35,336,192–107,962,368 bytes**（约 33.7–103.0 MiB） |
| Rust CPU | 按全机 16 逻辑核归一的观测区间均值 **1.1037%**、峰值 **1.6578%** |
| 最终历史覆盖 | 新鲜池 323、陈旧池 917；218 个历史有缺口，最大历史间隔约 3603.612 秒 |

Canary 完成时研究记录已自然增至 **2**，仍为 `INSUFFICIENT_SAMPLE`，未到期的 1h/2h/6h 不能当作已完成结果。[两条自然确认记录](evidence/v1-1-natural-signals.json) 属于同一个 SOL 池的不同规则，不是两个独立代币成功案例。之前记录数为 0 的真实案例和页面截图保留原采集时间，不回改旧证据。

这一负载与时长不是 QPS 基准、长期内存上界或 SLA。最后一次观察在顺序读取多个 API 时恰逢新扫描提交，个别来源成功时间比观察起始晚 0.429 秒，因此报告的 age 为 -0.429；原始证据保留该取样边界，不表示已验证未来数据。请求错误在同轮/后续请求恢复，观测级 `provider_recoveries=0`，没有把未捕获的状态转换虚构为恢复次数。

首次观察器在空通知计数 `{}` 上触发 PowerShell StrictMode 异常，已安全停止并修复；Rust 服务未停止。失败记录保留在 [观察器修复证据](evidence/v1-1-canary-observer-fix.json)。增加 4 项回归后从零重新计满 15 分钟，没有删除失败来制造全绿历史。

## 发布验收

- [PR #1](https://github.com/Callmecx/chainflow-intelligence/pull/1) 于 `2026-10-13T10:08:26Z` 正常合并，功能合并提交 `cf82cfeb35fb7791a6ee977d85a2d1d3587fe806`。
- PR 最终代码及证据提交 `a755d27` 的 [CI 38381129463](https://github.com/Callmecx/chainflow-intelligence/actions/runs/38381129463)：**SUCCESS**。已实际核对远端日志：62 Rust、31 单测、42 E2E、28 Windows 全通过。
- 合并提交的 [main CI 38381310811](https://github.com/Callmecx/chainflow-intelligence/actions/runs/38381310811)：**SUCCESS**。后续封版文档提交不改变上述功能源码；稳定 Tag 对应最终提交的状态可在仓库 Actions 继续核对。
- 功能合并提交的 [Pages 38381310726](https://github.com/Callmecx/chainflow-intelligence/actions/runs/38381310726)：**SUCCESS**，`10:10:04 UTC` 完成部署。[GitHub 元数据证据](evidence/v1-1-github-deployment.json)。
- 实际公开网址：[https://callmecx.github.io/chainflow-intelligence/](https://callmecx.github.io/chainflow-intelligence/)。`10:10:37–10:11:37 UTC` 两套真实浏览器检查 **PASS**，首页/JSON HTTP 200，快照生成 `10:09:28.777Z`，68 池（SOL 41 / BSC 27）、12 来源 OK、BSC logs UNSUPPORTED、provider_errors 为空。完整地址复制、详情 hash 刷新、研究导出、412px 手机布局及来源页通过，无浏览器错误、无本地 API 请求、无私人配置。
- 证据：[V1.1 面板](evidence/frontend-v11-real-pages/v11-real-browser-smoke.json)、[核心流程与来源明细](evidence/frontend-v11-pages-core/real-browser-smoke.json)、[网络/隐私边界](evidence/frontend-v11-real-pages/public-boundary.json)。公开研究记录为 0，与本机已积累 2 条的数据库相互隔离。
- 稳定版本：[v1.1.0](https://github.com/Callmecx/chainflow-intelligence/releases/tag/v1.1.0)。Tag 指向最终封版记录提交；该提交仅补文档及证据，未改变 `cf82cfe` 的功能源码。精确发布 SHA 可在 Tag / Release 核对；最终提交的 CI 与新快照部署同时在 Actions 留痕。

**本项目已完成本轮作品封版，停止自动功能开发。** 后续使用现有 [三分钟演示、源码阅读与简历描述](PORTFOLIO_GUIDE.md) 准备 Rust / Web3 求职，不自动开启 V1.2 / V2。

## 保留的边界

- Telegram：持久队列和 Mock 测试完成；**真实发送 PENDING**，本机未配置/未启用，没有以 Mock 冒充用户收到消息。
- 6h / 24h 自然长跑：**PENDING**。独立 Windows 脚本已提供，短 Canary 不能证明全天可用性。
- BSC event logs：**UNSUPPORTED**；公开 RPC 用于基础链状态与基准账户校验，未声称全链 Swap 索引。
- 免费来源有覆盖、速率和调度限制；存在 STALE 历史池。缺价、honeypot、Top10、聪明钱均保留 UNKNOWN。
- 研究不是收益承诺；自然样本不足不做规则有效性结论。默认采样不支持精确 3m K 线。
- 本轮费用 **0**；无付费 API、服务器、订阅、交易或私钥。完整边界见 [V1_1_LIMITATIONS.md](V1_1_LIMITATIONS.md)。

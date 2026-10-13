# 真实验收记录

验收日期：2026-10-13（Asia/Shanghai）。本记录分开列出真实网络、自动化 fixture、运行观察和远端发布证据；只有实际执行的检查才标记通过。公开行情值是记录时的历史样本，不是打开本文时的最新价格。

## 已发布地址与测试版本

| 项目 | 实际结果 | 可核验证据 |
| --- | --- | --- |
| 公开源码仓库 | 已创建并正常推送 | [Callmecx/chainflow-intelligence](https://github.com/Callmecx/chainflow-intelligence) |
| 初始已部署代码 | `4c69c650375806ff688e8f7fb1e80f16a87c3172` | [代码提交](https://github.com/Callmecx/chainflow-intelligence/commit/4c69c650375806ff688e8f7fb1e80f16a87c3172) |
| 初始 CI | success | [CI 38369339711](https://github.com/Callmecx/chainflow-intelligence/actions/runs/38369339711) |
| 初始 Pages 部署 | success | [Pages 38369339989](https://github.com/Callmecx/chainflow-intelligence/actions/runs/38369339989)；[状态留档](evidence/github-runs-initial.json) |
| 缺价禁止升级修复版本 | `b909b9ac796c43fb3f2d908740622ff8c8d4331e`，已通过 GitHub API 核对远端 main | [修复提交](https://github.com/Callmecx/chainflow-intelligence/commit/b909b9ac796c43fb3f2d908740622ff8c8d4331e) |
| 修复版 CI | success | [CI 38370068539](https://github.com/Callmecx/chainflow-intelligence/actions/runs/38370068539) |
| 修复版 Pages 部署 | success | [Pages 38370068645](https://github.com/Callmecx/chainflow-intelligence/actions/runs/38370068645) |
| 实际公开站点 | HTTP 200；`PUBLIC_SNAPSHOT` | [访问 Pages](https://callmecx.github.io/chainflow-intelligence/)；[HTTP 留档](evidence/pages-http.json) |
| 本轮费用 | 0；未采购 API、节点、服务器、域名或订阅 | 公开仓库标准 runner / Pages，无付费服务配置 |

上述 SHA 是固定的测试对象；之后更新文档、证据或前端模式校验的提交不应被误认为已经重新运行完整 Canary。最新远端状态可在 [Actions](https://github.com/Callmecx/chainflow-intelligence/actions) 查看。首次网页 HTTP 验证时间为 `2026-10-13T08:30:16.1305063Z`，实际快照生成于 `08:29:07.673Z`，含 65 个池（SOL 40 / BSC 25），`provider_errors=[]`。

## 自动化、脚本与数据完整性

| 验收项 | 真实结果 | 命令与证据 |
| --- | --- | --- |
| Rust 格式 | 通过 | `cargo fmt --manifest-path backend/Cargo.toml --all -- --check`；修复版 CI |
| Rust clippy | 通过，warnings denied | `cargo clippy --locked --manifest-path backend/Cargo.toml --all-targets -- -D warnings`；修复版 CI |
| Rust 单元/集成/fixture | 25 passed / 0 failed | `cargo test --locked --manifest-path backend/Cargo.toml --all-targets`；[实际输出](evidence/backend-test-results.txt) |
| Vue 类型检查与生产构建 | 通过 | `npm run build` 包含 `vue-tsc --noEmit` 和 Vite build；[前端证据](evidence/frontend-tests.json) |
| 前端单元测试 | 20 passed / 0 failed | Vitest，含新增 8 项模式真实性边界测试；[前端证据](evidence/frontend-tests.json) |
| 桌面 / 移动浏览器自动化 | 18 passed / 0 failed | 本地安装 Chrome，9 场景 × Desktop / Pixel 7 模拟；独立端口 5187，不复用其他项目 |
| 最终前端模式边界回归 | 6 passed / 0 failed；typecheck / build 通过 | 严格匹配 LOCAL_LIVE / PUBLIC_SNAPSHOT，拒绝 DEMO_FIXTURE / 未知 / 错配模式；修改后验证桌面和手机的模式、陈旧、缺失数据场景，未将先前 18 项结果冒称为本次小修复后的再次全量运行 |
| Windows 启动 / 诊断 | 实际成功 | 使用 8787 / 5178；所有脚本语法检查通过，PS5 doctor 退出 0；启动后真实本地浏览器与 Canary 均取得数据 |
| Windows 安全停止 / 重启 | 实际通过 | 本项目 API / 前端端口关闭，旧项目 5173 保留；新进程恢复数据并继续写入，[完整记录](evidence/windows-restart.json)；[隔离 PID 身份测试](evidence/windows-scripts.json) |
| Pages 失败快照处理 | 5 个离线场景通过 | 成功、采集失败、输出缺失、JSON 损坏、空结果；[证据](evidence/pages-failure-tests.json) |
| Canary 辅助函数 | 离线测试通过 | 空初始化数据、计数差值、重启后未知增量；[证据](evidence/canary-script-tests.json)，不是实际 Canary 的替代品 |
| SQLite 幂等 / 冷却 | 离线测试通过 | 跨源重复、重启重复、乱序、规则去重、观察名单满额重复写入；见 Rust 输出 |
| SQLite 完整性 | 实际 Canary 与重启后均为 ok，WAL | [实际 Canary](evidence/canary.json)；[重启验证](evidence/windows-restart.json) |
| 归档 / 恢复 | 临时 SQLite 测试通过 | 旧样本归档、同文件覆盖拒绝、恢复幂等；未把自动归档周期当成已经跑完 |

停止时间为 `08:35:02.9610995Z`，新服务运行起点为 `08:35:12.707Z`；重启验证在 `08:36:20.8304333Z` 记录样本由 1,261 增至 1,444，`health=OK`、`integrity_check=ok`，原有数据保留。该重启后的计数属于独立验收，不并入之前 Canary 的区间增量。

离线用例覆盖：错误链、非法地址、跨源大小写归一与去重、缺失市值/价格、极小及零流动性、乱序采样、429 / Retry-After（秒与日期）、503 后恢复、403 不重试、超时计数、缓存时间戳与共享限流、首次 6 分钟禁止确认、候选二次确认、陈旧数据不得升级、TTL、源失联提醒冷却、结果窗口不完整返回 UNKNOWN。fixture 均有明确标识，不作为公开实时行情或在线连接证据。

## 真实免费数据源

Rust `scan-once` 首次实际运行因禁用了 reqwest 的 Windows 系统代理支持而失败：`2026-10-13T08:15:30.572Z` 写入 0 样本失败快照并退出 1。启用 `system-proxy`、保留正常 TLS 证书校验后再次真实扫描成功；没有更换成假数据或关闭证书校验。[完整失败与复验记录](evidence/rust-online-scan.json) · [根因核对](evidence/provider-proxy-diagnosis.json)

复验快照生成于 `2026-10-13T08:17:20.388Z`：66 池（SOL 40 / BSC 26），0 provider errors。

| 来源 | Rust 复验 UTC | 实际连接 / 返回能力 | 程序限流 |
| --- | --- | --- | --- |
| DEX Screener profiles / boosts | 08:16:50.266Z / 08:16:57.399Z | HTTP 200，发现入口分别标明 PROFILE / BOOSTED | 发现共用 8 次/分钟 |
| DEX SOL / BSC 行情 | 08:16:57.751Z / 08:16:59.739Z | HTTP 200，真实代币池、价格、流动性、成交量、笔数；字段可缺失 | 行情共用 30 次/分钟，30 地址一批 |
| Gecko SOL new / trending | 08:16:50.278Z / 08:16:59.953Z | HTTP 200，两类池与 token 关系解析成功 | 所有 Gecko 接口共用 6 次/分钟，缓存 60 秒 |
| Gecko BSC new / trending | 08:17:10.006Z / 08:17:20.309Z | HTTP 200；保留源提供的 pool ID，不假定均为标准 V2 pair 地址 | 同一 Gecko 共享限额 |
| Solana 公共 RPC | 08:16:50.364Z | HTTP 200；WSOL getAccountInfo，slot `456372985`，commitment=confirmed | RPC 共用约 20 次/分钟，基准账户低频读取 |
| BSC 官方 RPC | 08:16:53.312Z | HTTP 200；chainId=`0x38`，block=`0x797802d`，WBNB decimals=18 | 一个 batch 含三种只读 RPC，使用共享 RPC 桶 |
| BSC event logs | 同轮状态记录 | UNSUPPORTED；没有承诺或实现连续全链 swap 流 | 未调用受限 eth_getLogs 绕过限制 |

来源字段、调用预算、缓存、平台偏差与原始 probe 见 [DATA_SOURCES.md](DATA_SOURCES.md)。额外 OHLCV/trades 方法已进行独立真实验证，UI 当前使用 SQLite 的真实采样曲线，未冒充逐笔 K 线。[补充端点记录](evidence/provider-additional-validation.json)

### 完整地址与字段核对样本

下列 6 个完整地址来自 Rust 成功扫描留档，UTC 日期均为 2026-10-13。金额单位 USD；UNKNOWN 是源字段缺失，不使用 FDV 代替市值。

| 链 / 抓取 UTC | 完整 token 地址 | 完整 pool 地址 / 查证 | MarketCap / FDV |
| --- | --- | --- | --- |
| SOL / 08:16:50.278Z | `2EcFpC35XeCMa6GvNupSkw8r2HoSmvpFNbDabbVJpump` | `2dpyTQVi6SoMeM9DGfzu23bq5sosj1kMWfSGzMKDNX7w` · [Gecko](https://www.geckoterminal.com/solana/pools/2dpyTQVi6SoMeM9DGfzu23bq5sosj1kMWfSGzMKDNX7w) | UNKNOWN / 3,181.479306 |
| SOL / 08:16:59.954Z | `2zMMhcVQEXDtdE6vsFS7S7D5oUodfJHE8vd1gnBouauv` | `ByKfqBGyHQj1hmEUouXfueLJEU1SHdJz3uMtd1yXGunS` · [Gecko](https://www.geckoterminal.com/solana/pools/ByKfqBGyHQj1hmEUouXfueLJEU1SHdJz3uMtd1yXGunS) | 496,889,168.830733 / 606,466,487.17309 |
| SOL / 08:16:59.954Z | `33jENUQnjoitkMFpvNJcSHyLAHFWJuHwT6E3E8CNpump` | `6iep3EgqksbZwgzMBCyAJtiVtVYkCgL9BnPBEwLR8AGQ` · [Gecko](https://www.geckoterminal.com/solana/pools/6iep3EgqksbZwgzMBCyAJtiVtVYkCgL9BnPBEwLR8AGQ) | UNKNOWN / 800,113.639785841 |
| BSC / 08:17:10.006Z | `0x0250675d8ec6438c8d8c72659c07e4c6e5159037` | `0x3dc69cae5e1d46160ee3972e76c89d3f60d1b1ae` · [Gecko](https://www.geckoterminal.com/bsc/pools/0x3dc69cae5e1d46160ee3972e76c89d3f60d1b1ae) | UNKNOWN / 172,030.1474 |
| BSC / 08:17:10.006Z | `0x0f2f40b8696171e73a479c89276e3cafa3ef4444` | `0x0f2f40b8696171e73a479c89276e3cafa3ef4444` · [Gecko](https://www.geckoterminal.com/bsc/pools/0x0f2f40b8696171e73a479c89276e3cafa3ef4444) | UNKNOWN / 24,179.23853 |
| BSC / 08:17:20.309Z | `0x1d28d989f9e3ccb8b15d0cec601734514f958e4d` | `0xe85bc115847d7106b6335d067c6d2cf9b837ce50626a1df3f01d0012681a5db4` · [Gecko](https://www.geckoterminal.com/bsc/pools/0xe85bc115847d7106b6335d067c6d2cf9b837ce50626a1df3f01d0012681a5db4) | 12,455,810.9944 / 4,552,380.68665762 |

所有来源 URL、价格原值及其抓取时间见 [Rust 扫描原始汇总](evidence/rust-online-scan.json)。最后两项保留了上游报告的相同 token/pool 地址或 bytes32 pool ID；这是来源数据的语义边界，不替换为猜测的 Pair 地址。MarketCap 与 FDV 的异常关系也照实保留，不宣称独立验证了流通供应。

## 15 分钟本机 Canary

真实运行证据：[canary.json](evidence/canary.json)。运行的是 `4c69c650375806ff688e8f7fb1e80f16a87c3172` 对应已构建服务，整个观察期间没有重启。后续 `b909b9a` 缺价禁止状态升级的小修复通过了 25 项 Rust 测试和 CI，但不属于这次 15 分钟连续运行覆盖范围。

| 项目 | 实际记录 |
| --- | --- |
| UTC 起止 | `2026-10-13T08:19:27.9616019Z` → `2026-10-13T08:34:28.8354189Z` |
| 持续时长 | 单调时钟记录 900.67 秒；达到 15 分钟 |
| 采样轮次 | 每 60 秒检查，共 16 次；16 次 API 成功，0 次失败 |
| 后台扫描 | 本次观察区间新增 7 轮，服务累计完成 8 轮 |
| 实际数据写入 | 区间新增 1,012 个采样；结束时累计 1,078 个 |
| 结束池数量 | 289 个 `(chain, token, pool)` 实体：SOL 140 / BSC 149；不是 289 个独立 token |
| Provider HTTP 请求 | 区间增量 88：DEX 46、Gecko 28、Solana RPC 7、BSC RPC 7；BSC 一个 batch 计一个 HTTP 请求 |
| 规则 / 观察信号 | 区间新增 407 条，累计 475 条；包含发现、状态变化和风险，不等同于 407 条高风险告警 |
| 来源异常 | Provider error 增量 0；BSC event logs 始终为 UNSUPPORTED |
| 本地 API 请求计数 | 区间增量 65，包含 Canary 与浏览器验证产生的请求 |
| 内存 | Windows 真实 WorkingSet64：21,291,008–28,270,592 bytes，约 20.3–27.0 MiB |
| 数据库 | 最终 `integrity_check=ok`；WAL；各次读取均为 ok |

以上增量由 Canary 首末实际可见计数相减。按样本 `fetched_at` 另行查询数据库会纳入观测结束时尚未提交的在途扫描，口径不同；不混合其风险分类计数来声称同一观察窗口的告警数量。

每源最后成功距最终观测时刻的年龄如下；这表示本机获取延迟，不代表链上交易到达延迟，也未消除上游缓存。

| 来源 | 请求增量 | 错误增量 | 最后成功 UTC | 结束时年龄（秒） |
| --- | ---: | ---: | --- | ---: |
| DEX profiles | 7 | 0 | 08:32:22.072Z | 125.89 |
| DEX boosts | 7 | 0 | 08:32:29.303Z | 118.66 |
| DEX SOL 行情 | 19 | 0 | 08:32:34.028Z | 113.94 |
| DEX BSC 行情 | 13 | 0 | 08:32:38.192Z | 109.77 |
| Gecko SOL new | 7 | 0 | 08:32:21.996Z | 125.97 |
| Gecko SOL trending | 7 | 0 | 08:32:32.003Z | 115.96 |
| Gecko BSC new | 7 | 0 | 08:32:41.984Z | 105.98 |
| Gecko BSC trending | 7 | 0 | 08:32:52.004Z | 95.96 |
| Solana RPC / mint 基准验证 | 7 | 0 | 08:32:22.274Z | 125.69 |
| BSC RPC / contract 基准验证 | 7 | 0 | 08:32:25.149Z | 122.82 |

## 真实浏览器与网页验收

| 检查 | 本地 LOCAL_LIVE | GitHub Pages PUBLIC_SNAPSHOT |
| --- | --- | --- |
| 真实页面 / 数据 HTTP | 200 / 200 | 200 / 200 |
| 真实 SOL / BSC 筛选 | 55 / 50 行 | 40 / 25 行 |
| token 详情与 hash 刷新 | 通过 | 通过 |
| 完整地址复制 | 实际 clipboard 核对通过 | 实际 clipboard 核对通过 |
| 浏览器 / 来源外链 | href 指向真实对应地址 | href 指向真实对应地址 |
| 手机视图 | 412px，首页和详情无横向溢出 | 412px，首页和详情无横向溢出 |
| 来源状态与数据 UTC 时间 | 显示实际状态与时间 | 显示快照模式、实际生成时间 |
| 浏览器运行时错误 | 0 | 0 |

本地检查于 `08:21:27.598Z–08:21:30.272Z` 完成：[机器记录](evidence/frontend-real-local/real-browser-smoke.json)、[桌面](evidence/frontend-real-local/real-desktop-home.png)、[手机](evidence/frontend-real-local/real-mobile-home.png)。Pages 检查于 `08:30:17.093Z–08:30:28.170Z` 完成：[机器记录](evidence/frontend-real-pages/real-browser-smoke.json)、[桌面](evidence/frontend-real-pages/real-desktop-home.png)、[手机](evidence/frontend-real-pages/real-mobile-home.png)。真实详情验证地址为 `2zMMhcVQEXDtdE6vsFS7S7D5oUodfJHE8vd1gnBouauv`。

空数据、SOURCE_ERROR、STALE、未知数值与非法输入由明确标识 TEST_FIXTURE 的 18 场景浏览器测试覆盖；没有为了让公开站点失效而替换线上真实数据，因此这部分不是现场上游断网实验。外链检查验证对应 href，不等于本项目能保证第三方浏览器站点始终可用。

## 已完成范围与剩余限制

- 已完成 Rust Provider、共享限流/缓存/退避、真实 SOL/BSC 发现与精确地址查询、SQLite 单写入/WAL、观察名单、规则状态机/风险证据、采样历史与结果框架、REST API、中文 Vue 看板、Windows 启停诊断、CI 与公开定时快照 Pages。
- 初期系统代理差异和手机横向溢出均已修复并复验。已有项目占用的 5173 未被停止或修改，本项目采用 5178，Playwright 使用独立 5187。
- 没有执行实盘交易、连接钱包或读取私钥；Telegram 未接入。honeypot / Top10 / 聪明钱为 UNKNOWN；不能据行情或基准 RPC 可用推断任意 token 安全。
- 15 分钟 Canary 不足以验收 1h / 2h / 6h 真实完整结果，不足以证明盈利、高可用或长期内存稳定。未完成区间保留 UNKNOWN；后续优先做 6 小时以上观察及研究对照校验。
- 超时、429、503、断点恢复与数据库重启幂等已通过 fixture/临时 SQLite 测试；本轮没有对真实互联网进行拔网线/人为断网恢复实验。WebSocket 未启用，断线重连测试不适用。
- 服务内每 720 轮自动归档与按启动分段日志已实现；未实际跑完自动归档周期，未实现运行中按日日志轮转。归档恢复不是完整 SQLite 备份恢复的替代。
- Pages 目标每半小时扫描，当前验证覆盖 push 部署；尚未等待长期 schedule 周期证明持续准点运行。GitHub 调度无严格 SLA，页面陈旧状态按实际时间判断。
- DEX / Gecko 有平台覆盖与推广偏差、缓存和限流；BSC 全链 logs / swap 流不支持。结果研究为采样研究，固定地址哈希对照不等于严格随机收益试验。更完整边界见 [LIMITATIONS.md](LIMITATIONS.md)。

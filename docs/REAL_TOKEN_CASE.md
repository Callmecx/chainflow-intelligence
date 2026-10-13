# 真实案例：Wrapped SOL 单池观察

本案例来自运行中的 V1.1 本地服务，只读抓取于 **2026-10-13 09:46:42 UTC（北京时间 17:46:42）**。代码提交为 `90251caaea2968c6efa42068edf470844b9a4920`。自然行情没有产生正式确认，不调整阈值、不补造事件或收益。机器可核查的白名单摘录见 [JSON 证据](evidence/v1-1-real-token-case.json)。

## 三分钟讲解顺序

**第一分钟：身份与数据路径。** 从代币详情的完整地址识别 Wrapped SOL，展示 `BASELINE` 标签：它是已知公开基准，用于检验真实数据连通性，不能被包装成新币发现成绩。价格和流动性来自 DexScreener；Solana RPC 另行验证 mint 账户。Rust 持续采样写入 SQLite，Vue 读取本地 API。

**第二分钟：展示数据与判断依据。** 对照下面五次真实采样，解释采样时间、行情来源和流动性。当前分析是 `SAMPLED`，5m/15m 结构可计算，3m 显示 `INSUFFICIENT_RESOLUTION`。条件分为 20/100，只有流动性维持条件命中；它不是上涨概率，也没有触发正式确认。

**第三分钟：展示可信边界。** 打开事件时间线，指出当前 `INVALIDATED` 继承自升级前的历史事件。旧记录缺少规则版本和源时间，不伪装成 V1.1 产出的完整证据。再查看研究面板：自然正式确认记录为 0、`INSUFFICIENT_SAMPLE`，因此没有可展示的 1h/2h/6h 收益。说明 Telegram 真实投递和 6h 持续验收仍是 `PENDING`，与自动化测试区分。

## 实体与接口

| 字段 | 真实观察值 |
| --- | --- |
| 链 / 名称 | Solana / Wrapped SOL（行情符号 SOL） |
| 完整 mint | `So11111111111111111111111111111111111111112` |
| 完整池地址 | `Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE` |
| 本地详情接口 | `/api/tokens/solana/So11111111111111111111111111111111111111112`，HTTP 200 |
| 当前源与源时间 | `dexscreener`，`2026-10-13T09:45:28.559Z` |
| 首次本地观察 | `2026-10-13T08:18:30.132Z` |
| 抓取时已保存采样 | 45 条，包含升级前真实历史 |
| 来源池创建时间 | `2023-06-30T14:20:58+08:00`；不是新池 |
| 当前状态 | `INVALIDATED` / `RISK`；本次服务启动后该池新增规则事件 0 |

可以交叉打开 [DexScreener 单池页面](https://dexscreener.com/solana/Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE)、[公开单池 API](https://api.dexscreener.com/latest/dex/pairs/solana/Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE) 和 [mint 浏览器](https://solscan.io/token/So11111111111111111111111111111111111111112)。09:45:32 UTC 的独立单池 API 请求返回 HTTP 200，mint / pool 一致，价格 $107.20、流动性 $27,866,153.97；这是另一请求的观察结果，没有替换本地历史。之后打开链接时行情可能变化。

## 最近五次采样

以下时间均为 2026-10-13 UTC，来源均为 DexScreener。滚动 5m 买卖笔数存在窗口重叠，不相加为独立交易量，也不解释为净资金流。

| 源时间 | 价格 USD | 流动性 USD | 滚动 5m 买入 / 卖出笔数 |
| --- | ---: | ---: | ---: |
| 09:37:21.392 | 107.12 | 27,867,698.86 | 435 / 152 |
| 09:39:21.862 | 107.15 | 27,857,125.98 | 404 / 168 |
| 09:41:21.299 | 107.41 | 27,907,847.91 | 237 / 80 |
| 09:43:28.653 | 107.27 | 27,879,458.44 | 201 / 264 |
| 09:45:28.559 | 107.20 | 27,866,153.97 | 150 / 267 |

这五条不能证明采样间没有更高或更低价格。服务以约 120 秒间隔轮询，5m/15m 分析允许边界采样容差；展示的是采样结构，不能称为逐笔成交或精确 OHLCV。

## 规则事件与 UNKNOWN

时间线保留 `DISCOVERED`（08:18:30 UTC）、`STATE_WATCH`（08:20:29 UTC）、`SELL_COUNT_IMBALANCE` / `STATE_INVALIDATED`（08:49:20 UTC）以及再次卖出笔数失衡（09:15:21 UTC）。08:49 的证据是滚动 5m 买入 12、卖出 177；09:15 为 13 / 198。它们均为升级前记录，`rule_version` 和 `source_at` 为空，不能追溯成新版规则运行结果。当前 V1.1 未覆盖这些历史事件，详情说明“先前风险已使本轮观察失效，不自动重新确认”。

当前 `market_cap_usd` 与 `fdv_usd` 均为空；市值显示 `UNKNOWN`，FDV 不替代市值。`HONEYPOT`、`TOP10`、`SMART_MONEY` 未独立验证，均为 `UNKNOWN`。旧池年龄和缺失市值不满足新池确认条件。当前 `breakout=false`、`multi_period=false`、`activity_recovery=false`；`sampled-v1.1.0` 的 20 分仅来自“5m 流动性维持”。

## RPC 的角色与验收边界

抓取时健康接口 HTTP 200、`status=OK`、数据库 schema 2；服务启动 `09:43:20.324Z`，最近完整扫描 `09:45:53.04Z`。这些是抓取时点状态，不是持续可用性承诺。

Solana RPC 在 `09:45:20.991Z` 返回 WSOL 的 `getAccountInfo`，`slot=456397402`、`commitment=confirmed`。这是账户只读验证，没有交易签名，不能证明上述池流动性或每笔 swap。BSC RPC 同轮验证 `eth_chainId=0x38`、区块读取和 WBNB `decimals=18`，没有参与 SOL 报价；免费 BSC 全链事件日志仍标为 `UNSUPPORTED`。

`/api/research` 当时返回 `total_records=0`、`min_sample_size=30`、`INSUFFICIENT_SAMPLE`；该池研究记录也为 0。1h/2h/6h 的所有窗口计数为 0，而不是已经完成且收益为 0。正式信号产生后才开始前瞻记录，旧版事件不回填。

本案例证明真实接口、持续样本与诚实状态展示。它不替代最终 Canary 报告、不证明 Telegram 已真实投递，也不证明 6h/24h 运行通过。JSON 未包含私人自选列表、批量请求中的其他代币、PID、数据库路径或凭据。

## Canary 末新增自然确认

随后只读查询 `/api/research`，自然正式研究记录已经从 0 增至 **2**，整体仍为 `INSUFFICIENT_SAMPLE`（最低样本量 30）。这不是更改前文结果：前文及 WSOL JSON 是 09:46 UTC 的历史时点，新增记录发生在之后。精确捕获时间、API 返回与窗口状态见 [自然信号白名单证据](evidence/v1-1-natural-signals.json)。

本次 API 捕获时间为 **10:03:54.392 UTC**，当时总数确为 2。按 [Canary 报告](evidence/v1-1-canary/soak-20261013T094648Z-a1be38.json) 的实际观测区间 **09:46:49.415—10:01:54.116 UTC** 逐条比较 `trigger_at`，下面两条均发生在该区间内，区间结束后至本次捕获的新增正式研究记录为 0。API 捕获发生在 Canary 结束之后；今后继续产生的记录不计入这份固定时点证据。

两条记录属于**同一 Solana 代币、同一池的两个规则事件**，不是两个独立代币案例：

- chain：`solana`
- 完整 token：`2v52Jo1yC5MVaJxa6ZzESEvcdbs6yUhjgNefXLmvpump`
- 完整 pool：`EtY35iYEu9YRMbiw6mCrFt4PrEnvAdBqtcTLW8GAqmED`
- 来源：`dexscreener`；规则版本均为 `sampled-v1.1.0-e9f1b342308e3a3`；[公开单池链接](https://dexscreener.com/solana/EtY35iYEu9YRMbiw6mCrFt4PrEnvAdBqtcTLW8GAqmED)。

| 规则 | 正式触发时间 UTC | 证据源时间 UTC | 触发基准价 USD | 基准流动性 USD |
| --- | --- | --- | ---: | ---: |
| `STATE_CONFIRMED` | 2026-10-13 09:54:03.235 | 2026-10-13 09:53:29.279 | 0.00006618 | 21,169.30 |
| `MULTI_PERIOD_5M_15M` | 2026-10-13 09:59:53.704 | 2026-10-13 09:59:30.532 | 0.00007473 | 22,109.61 |

捕获时两条记录的 **1h / 2h / 6h 全部为 `PENDING`**，收益、最大有利/不利变化、最大回撤与流动性变化全部为 `null`。第一条的目标时间为 UTC 10:54:03、11:54:03、15:54:03；第二条为 10:59:53、11:59:53、15:59:53，各有 180 秒容差，目标时间及容差到达后才评估。没有将后续涨价当成已完成收益，也没有构造未来结果。这次补记仅读取 API、保存白名单证据，没有修改阈值、注入样本或重启服务。

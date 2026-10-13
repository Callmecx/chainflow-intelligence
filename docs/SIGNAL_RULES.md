# 观察标签、风险规则与状态机

规则用于可解释观察；任何标签均不构成交易建议。配置入口为 `CHAINFLOW_RULES_JSON`，由 `backend/src/config.rs` 校验。修改后重启本机服务。公开快照使用默认配置。

## 默认参数

| 字段 | 默认值 | 意义 |
| --- | ---: | --- |
| min_liquidity_usd | 3000 | 候选池最低流动性 |
| min_market_cap_usd | 5000 | 已知市值下限 |
| max_market_cap_usd | 200000 | 已知市值上限 |
| max_pool_age_hours | 4 | 池年龄上限 |
| observation_ttl_seconds | 7200 | 本轮观察有效期 |
| confirmation_delay_seconds | 360 | 最早确认候选时间，不可低于 6 分钟 |
| cooldown_seconds | 900 | 相同 chain/token/pool/规则冷却 |
| liquidity_drop_pct | 30 | 相邻有效采样流动性跌幅阈值 |
| drawdown_pct | 20 | 相邻有效采样价格回撤阈值 |
| max_sample_gap_seconds | 300 | 允许的最大样本间隔 |
| outcome_tolerance_seconds | 180 | 结果时点允许的前后容差 |
| research_min_samples | 30 | 每组低于此有效样本数显示不足 |
| structure_weights | [30,25,25,20] | 低点、多周期、平台突破、流动性四因子权重 |
| breakout_buffer_pct | 1 | 突破/重夺平台的百分比缓冲 |
| recovery_seconds | 900 | 风险后恢复观察的最短冷却 |

MarketCap 缺失不能用 FDV 替代；池创建时间缺失不能推定为新池。默认阈值不代表已验证的获利策略。100K–1M、1M–2M 等视图仅为筛选范围，不自动改变风险事实。

## 状态与标签

`DISCOVERED → WATCH → CONFIRMATION_CANDIDATE → CONFIRMED`，遇风险可 `INVALIDATED`，TTL 到期为 `EXPIRED`。风险提醒不受首次 6 分钟确认延迟限制。

- `NEW`：首次真实获取，等待后续采样。
- `ACTIVE`：持续观察；不一定符合完整候选阈值。
- `MOMENTUM_WATCH`：连续采样充足且满足阈值时，窗口价格变化达到规则要求。
- `RANGE_WATCH`：充足采样窗口价格变化较小的观察标签。
- `REVIVAL_WATCH`：从窗口低点回升的观察标签；不推断庄家行为。
- `RISK`：独立市场风险事件；流动性急降、价格回撤、连续回撤或假突破使结构失效。笔数失衡、量价背离和跳涨提示风险并阻止本轮新确认，不删除先前确认的历史。
- `EXPIRED`：观察 TTL 到期；历史仍可保留作研究。

至少 4 个价格有效且按时间递增的样本，并满足窗口覆盖、最大采样间隔，才可评估相应采样结构。5m / 15m / 1h 的不足窗口写入 `INCOMPLETE_DATA`。数据陈旧、缺失或乱序不得升级确认。确认候选需后续有效观察才能进一步确认，单点跳涨不足以证明趋势。

## 风险证据

| 规则 | 判断与限制 |
| --- | --- |
| LIQUIDITY_DROP | 对间隔有效的相邻实际样本比较流动性，跌幅达到阈值；不将除零结果当风险强度。 |
| DRAWDOWN | 相邻有效样本价格回撤达到阈值；不声称覆盖采样间极值。 |
| SELL_COUNT_IMBALANCE | 5m 总笔数至少 20 且卖出笔数占比至少 85%；仅笔数结构。 |
| VOLUME_PRICE_DIVERGENCE | 滚动 24h 量上升伴随采样价格回落；不是独立短时新增成交量。 |
| 来源异常/陈旧 | 显示来源错误和实际采样时间，不以旧值伪装实时。 |
| 同名与地址 | 名称/符号不是唯一标识，操作依据完整 chain + address；非法地址拒绝进入查询/观察名单。 |

信号保留 `WHY / EVIDENCE / INCOMPLETE_DATA / INVALIDATION`、来源、池地址、UTC 时间和状态变化。相同 chain/token/pool 与规则冷却去重，同币不同池独立，状态与普通高风险事件分开显示。历史无数据时显示 UNKNOWN。

## V1.1 同源结构与确认方案

`structures.rs` 的结构均为 SAMPLED 近似统计，不构造虚拟 OHLCV。3m 要求间隔≤60秒、5m 要求≤150秒、15m 要求≤300秒，且至少4点、有完整窗口覆盖、有效价格、同一来源；不满足时 `INSUFFICIENT_RESOLUTION`。默认120秒扫描因此不声称3m形态可用。

- `STATE_CONFIRMED`：保留连续5m观察方案，至少6分钟观察、同源连续有效采样，候选之后再确认。
- `MULTI_PERIOD_5M_15M`：5m/15m两窗口后半段低点均比前半段高至少1%，各窗口价格变化分别达到2%/3%；仍需新池/市值/流动性门槛和新鲜数据。
- `PLATFORM_BREAKOUT`：先前15m样本参考区间幅度≤8%，排除最新两点后构造平台；突破默认1%只产生WATCH，不单独当正式确认。
- `PLATFORM_RETEST` / `RETEST_RECLAIM`：先保存同源突破平台，再观察回到平台附近，随后有效样本重新站上平台才确认。先检查假突破，既有引用不能被新突破覆盖。
- `FALSE_BREAKOUT`：已有突破后跌破平台3%，使状态INVALIDATED，同轮不得保留任何正式确认事件。
- `ACTIVITY_REVIVAL`：同源窗口前半段滚动5m笔数低、后半段至少两次恢复，作为WATCH；不是独立成交量或聪明钱判断。
- `PRICE_JUMP_UP`：相邻有效样本上涨达到 `drawdown_pct`（默认20%），独立RISK并禁止本轮新确认，不把跳涨直接当机会。
- `CONTINUOUS_DRAWDOWN`：严格最近5分钟至少4个同源有效样本且连续下降，累计达到回撤阈值才失效；默认120秒扫描通常只有3点，因此不能假装该细粒度条件已经满足。

`INVALIDATED` 可在无新风险、连续有效样本、市值/流动性门槛及恢复冷却满足后进入 `RECOVERY_WATCH`；保留历史失效，并重新经过确认过程。TTL到期仍EXPIRED。未来时间、来源切换、缺价、断档均阻止正式确认；DATA_FAULT从未转换成撤资风险。

四因子按可配置权重归一为0–100条件覆盖分，缺少证据不加分。分数不等于上涨概率，也不作为单独确认条件。规则版本带配置指纹，三种确认机制分别记录结果，不混为经过验证的策略。

## 结果研究

V1.1正式信号基准、容差、COMPLETE/MISSING/CENSORED分母及导出规则见 [SIGNAL_OUTCOMES.md](SIGNAL_OUTCOMES.md)。下面是保留的V1首次观察基线，不能代替逐信号研究。

以首次观察价为简单基线，并保留固定地址哈希选样作为研究对照。对随后 1h / 2h / 6h 的已覆盖真实采样区间计算最大正向/负向变化。窗口尚未结束、价格缺失或中途采样间隔过大时结果为 UNKNOWN。数值只代表 SAMPLED 区间，不是可成交收益、逐笔最大涨跌幅、滑点或手续费调整后的回测；固定哈希对照不等于严格随机试验，不宣称策略优势。

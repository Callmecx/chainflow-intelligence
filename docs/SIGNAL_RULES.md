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
| cooldown_seconds | 900 | 相同代币/规则冷却 |
| liquidity_drop_pct | 30 | 相邻有效采样流动性跌幅阈值 |
| drawdown_pct | 20 | 相邻有效采样价格回撤阈值 |
| max_sample_gap_seconds | 300 | 允许的最大样本间隔 |

MarketCap 缺失不能用 FDV 替代；池创建时间缺失不能推定为新池。默认阈值不代表已验证的获利策略。100K–1M、1M–2M 等视图仅为筛选范围，不自动改变风险事实。

## 状态与标签

`DISCOVERED → WATCH → CONFIRMATION_CANDIDATE → CONFIRMED`，遇风险可 `INVALIDATED`，TTL 到期为 `EXPIRED`。风险提醒不受首次 6 分钟确认延迟限制。

- `NEW`：首次真实获取，等待后续采样。
- `ACTIVE`：持续观察；不一定符合完整候选阈值。
- `MOMENTUM_WATCH`：连续采样充足且满足阈值时，窗口价格变化达到规则要求。
- `RANGE_WATCH`：充足采样窗口价格变化较小的观察标签。
- `REVIVAL_WATCH`：从窗口低点回升的观察标签；不推断庄家行为。
- `RISK`：触发风险，本轮观察失效。
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

信号保留 `WHY / EVIDENCE / INCOMPLETE_DATA / INVALIDATION`、来源、池地址、UTC 时间和状态变化。相同 token 与规则冷却去重，状态与普通高风险事件分开显示。历史无数据时显示 UNKNOWN。

## 结果研究

以首次观察价为简单基线，并保留固定地址哈希选样作为研究对照。对随后 1h / 2h / 6h 的已覆盖真实采样区间计算最大正向/负向变化。窗口尚未结束、价格缺失或中途采样间隔过大时结果为 UNKNOWN。数值只代表 SAMPLED 区间，不是可成交收益、逐笔最大涨跌幅、滑点或手续费调整后的回测；固定哈希对照不等于严格随机试验，不宣称策略优势。

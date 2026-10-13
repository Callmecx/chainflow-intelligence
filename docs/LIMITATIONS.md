# 能力边界与未完成项

本文描述 V1.1 当前能力。项目是只读研究与求职展示工具，封版范围为 PORTFOLIO_READY。公开数据存在平台选择、缓存、网络和限流偏差；观察标签只是规则解释，没有经过收益验证。新增模块的详细边界见 [V1_1_LIMITATIONS.md](V1_1_LIMITATIONS.md)。

| 项目 | 当前边界 |
| --- | --- |
| 全部新币捕获 | 不支持。Gecko 收录池和 DEX profiles / boosts 只是部分覆盖。 |
| 真实实时性 | 本机目标 120 秒轮询；免费端点缓存、退避、网络耗时会增加延迟。 |
| Pages 持续监控 | 不支持。每半小时目标扫描，浏览器只有该次快照；调度可能延迟或停用。 |
| 公开站点自选监控 | 不支持。网页收藏仅保存在浏览器，本机观察名单才参与连续采样。 |
| MarketCap / FDV | 上游缺失时 UNKNOWN。FDV 不是市值，多池值不能叠加。 |
| K线与成交 | 本地采样曲线仅 SAMPLED。未接逐笔全链数据流，不能反映采样间的最高/最低价。 |
| 3m / 5m / 15m 结构 | 要求同源、有序、有效且覆盖完整的采样。默认 120 秒轮询无法支持 3m 所需精度；不足时显示 INSUFFICIENT_RESOLUTION，不插值确认。 |
| 买卖笔数 | 表示笔数，不能解释为净资金流或真实买卖金额。 |
| honeypot / Top10 / 聪明钱 | UNKNOWN，未完成独立验证。公开 RPC 连通不代表代币合约安全。 |
| Solana 全链监听 | 未启用 WebSocket 全链监听。公共 RPC 无高频全链生产保障。 |
| BSC 全链事件 | 官方免费 RPC 的 `eth_getLogs` 能力受限；标记 UNSUPPORTED，不承诺 swap 全量覆盖。 |
| 叙事/社交 | 不依赖 X/Twitter 付费搜索，不生成未经证实的叙事或钱包标签。 |
| 1h / 2h / 6h 研究 | V1.1 为新产生的正式确认信号冻结基准、规则版本和证据；按目标时间与容差区分 PENDING / COMPLETE / MISSING / CENSORED。缺失/删失保留分母，样本不足为 INSUFFICIENT_SAMPLE；短 Canary 无法验收 6h 自然结果。 |
| 对照研究 | V1 的首次观察价与固定地址哈希选样是描述性基线；V1.1 增加按正式信号/规则版本分组的前瞻统计。两者都不是严格随机试验、可成交收益或策略有效性证明。 |
| Telegram | 本机持久队列、凭证加载、过滤、去重、冷却、重试和重启恢复已实现；真实投递 PENDING，mock 不等于实际送达。超时可能 UNCERTAIN，无法保证 Exactly Once。默认关闭，不影响行情监控。 |
| 跟踪覆盖 | 本机私人关注最多 60 个；持续扫描每链最多 80 个优先跟踪地址，DEX 每链总体最多 90 地址。待研究/候选优先也不能保证全部池持续覆盖。 |
| 高可用 | 本机休眠、进程停止或网络中断即停止采集；无外部付费节点兜底。 |
| 公网 API | 未提供认证/多用户隔离，服务仅供回环访问；不能直接作为商业公网 API。 |

未知或失败状态不会自动升级成确认：首次观察至少 6 分钟后才可能按规则进入确认流程，普通风险提示可立即发生。TTL 默认 2 小时。WATCH、CONFIRMATION、RISK 与 DATA_FAULT 分开记录，数据源失联不推断成市场撤资；确认标签不代表安全或建议买入。

6h / 24h 自然长跑与 Telegram 真实投递仍为 **PENDING**。按本次求职作品封版范围，它们不是稳定版发布的硬门槛，也不能因此写成已通过。Windows 独立运行与报告方法见 [SOAK_RUN_GUIDE.md](SOAK_RUN_GUIDE.md)。

GitHub Pages 只用于非商业开源演示和公开研究；GitHub 的使用规则限制把 Pages 当作商业交易或 SaaS 托管。[官方限制](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits)

当前测试、发布状态和 PENDING 项见 [V1_1_ACCEPTANCE.md](V1_1_ACCEPTANCE.md)；[ACCEPTANCE.md](ACCEPTANCE.md) 保留 V1 历史记录。本文不代替测试结果。

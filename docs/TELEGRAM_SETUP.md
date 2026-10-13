# Telegram 中文提醒：配置、可靠性与验收

Telegram 仅在本机 `serve` 持续监控中使用；公开 GitHub Pages / `scan-once` 不发送通知、不公开通知队列、收件聊天或任何凭据。费用为 0，发送请求明确设置 `allow_paid_broadcast=false`，不使用付费广播。

**真实发送验收状态：PENDING。** 自动化测试使用 mock transport 和本机 HTTP fixture，不代表消息已送达 Telegram。本轮没有向用户索取聊天中的密钥；只有用户在自己电脑配置后，真实 Bot API 返回 `ok=true` 且有效 `message_id`，并在对应聊天确认收到，才能完成真实投递验收。健康页的 `DELIVERED_RECORDED` 表示数据库有成功 API 响应记录，不等于用户已阅读消息。

## Windows 配置

1. 在 Telegram 官方 [@BotFather](https://t.me/BotFather) 创建自己的 bot。Token 只保存在本机，不粘贴到 Codex/聊天、截图或公开问题单。
2. 打开自己的 bot 并发送 `/start`。如果使用私人群组，把 bot 加入目标群组并允许发送消息；chat ID 由用户自己的 Bot API `getUpdates` 响应或 Telegram 管理信息确认。不要把 bot token 提供给第三方“查询 chat ID”网站。
3. 在项目根目录复制 `.env.example` 为 `.env`，在本地编辑器填写：

   ```dotenv
   TELEGRAM_ENABLED=true
   TELEGRAM_BOT_TOKEN=在本机填入BotFather提供的Token
   TELEGRAM_CHAT_ID=在本机填入目标ChatID
   ```

   `.env` 已在 Git 忽略范围内。三个值不要写到 README、GitHub Pages、CSV、运行报告或截图。Windows 启动脚本只把允许的键加载进当前进程及子进程，不修改机器级环境。

4. 按 [README](../README.md) 启动本机服务。只有 `TELEGRAM_ENABLED=true`（或 `1`）并且 token、chat ID 均存在，才构建 Telegram transport；缺配置/未启用时其他行情功能正常工作，历史信号也不会悄悄积压待启用后补发。
5. 本地健康页只显示已配置/启用、队列计数、最近发送状态和固定错误分类。等待新的正式确认或有证据的风险事件；在对应 Telegram 聊天确认收到了完整中文卡片，再核对健康页 `SENT` 计数。尚未自然触发可发送事件时仍为 PENDING，不能为验证而伪造市场信号。

如果直接使用 `cargo run ... -- serve`，需要在启动它的 PowerShell 会话中自行配置环境变量；Cargo 本身不会加载 `.env`。不要把真实凭据放在 shell 参数里或录入共享终端日志。缺少正确 chat ID、用户未先 `/start`、bot 被移出群组或发送权限不足，都可能导致消息不能送达。

## 发送内容与范围

默认发送新产生的 `CONFIRMED` + `CONFIRMATION` 正式事件和有证据的 `RISK` 市场风险事件。普通 WATCH 默认不发送；`CONFIRMATION_CANDIDATE` 也不发送。来源时间超过 300 秒、未来时间明显异常、实体链/代币/池不一致、没有证据的事件会被拒绝。规则确认不是买入建议，也不是上涨概率。

中文消息包含名称、symbol、所属链、完整 mint/合约、池地址、首次发现 UTC、采集 UTC、事件 UTC、价格、MarketCap、FDV、池流动性、24h 池成交量、状态、原因、证据、缺失/风险因素、失效条件和官方 DEX/Gecko 池详情链接。缺失指标显示 UNKNOWN，市值与 FDV 分开。使用纯文本而不是 HTML/Markdown parse mode，防止第三方币名被解释为格式指令；完整地址保留，长原因文本限长。

`DATA_FAULT` 使用独立卡片，至少连续 3 次失败后才可入队，并采用独立冷却域。数据故障不推断成代币流动性流失。当前持续故障判断由监控服务提供，RPC/聚合器错误不会当作市场风险卡片发送。

## 队列与状态

队列表和发送尝试表都位于现有 SQLite，通过已有单 writer 执行；通知模块不新建数据库或独立写线程。HTTP 请求在 writer 之外等待，完成后再回写结果。

| 状态 | 含义与处理 |
|---|---|
| PENDING | 新事件已经可靠入队，尚未发送。健康页 `verification=PENDING` 则表示尚无成功发送验收记录；两个字段不是同一含义。 |
| SENDING | writer 已原子领取，网络结果尚未落库。 |
| SENT | 已收到 Telegram `ok=true` 和有效 message_id。状态页不会公开 message_id、消息正文或 chat ID。 |
| RETRY | 已知可重试失败，达到下一次尝试时间再发。 |
| UNCERTAIN | 超时、无法确认网络结果、异常成功响应，或领取后进程重启；请求可能已经送达，默认不自动重发。 |
| FAILED | 永久失败，或超过最多 4 次尝试。 |
| EXPIRED | 信息已过时，不再投递，防止恢复后历史提醒集中刷屏。 |
| SUPPRESSED | 相同链/代币/池/规则/类别处于 15 分钟冷却期。 |

事件 ID 唯一约束提供去重，同链不同池与跨链实体隔离；重复事件不能再次入队。默认每次发送至少间隔 5 秒，优先级为市场风险 > 正式确认 > 数据故障 > 显式开启的观察事件。全局发送时钟与冷却记录持久化，重启不重置为无限速。

默认入队 TTL 10 分钟，市场信号同时受采集数据最多 5 分钟有效期约束；领取时再次过期检查。启动恢复把未完成的 SENDING 标记 UNCERTAIN，过期 PENDING/RETRY 标记 EXPIRED，剩余近期队列依旧按全局速率发送，不集中刷屏。UNCERTAIN 只能人工核对 Telegram 后决定后续处理；当前没有“自动补发不确定消息”按钮。

## 失败、退避与凭据撤销

- 429：读取 JSON `parameters.retry_after` 和 HTTP `Retry-After`（秒或 HTTP 日期），至少等待要求时长；该冷却对整个 bot 队列生效。
- 5xx、明确连接失败：指数退避，最多 4 次尝试。默认退避从 10 秒开始，设有上限；若要求等待时间超过消息有效期，该事件直接过期。不会缩短 Telegram 指定的 Retry-After。
- 请求或响应超时：UNCERTAIN，不自动重试。Telegram sendMessage 没有本项目可依赖的幂等键，因此不能承诺绝对 Exactly Once；其他服务器失败重试也有重复风险。
- 401：`AUTH_REJECTED`；403：`DESTINATION_REJECTED`；400：`BAD_REQUEST`。不把原始响应 description、请求 URL 或异常字符串写到状态/日志，避免泄露 token 路径和聊天信息。
- 停用：本机 `.env` 设置 `TELEGRAM_ENABLED=false` 并重启服务。撤销/更换：在 [@BotFather](https://t.me/BotFather) 撤销旧 token、生成新 token，只在本机替换，随后重启。更换接收聊天时一并更新 `TELEGRAM_CHAT_ID`。排查前先保持关闭，已有数据库的市场历史不会因此删除。

SQLite 中的通知正文包含本机观察的代币和事件信息，属于本地运行数据，不能上传仓库或 Pages。发布扫描不读取个人通知队列。备份数据库应按现有 SQLite 备份流程处理，分享诊断仅分享固定状态/计数。

## 官方协议依据与测试边界

[Telegram sendMessage](https://core.telegram.org/bots/api#sendmessage) 定义必需的 chat_id/text、消息长度上限与成功 Message 返回；[ResponseParameters](https://core.telegram.org/bots/api#responseparameters) 定义 retry_after 秒数；[官方 Bot FAQ 限流](https://core.telegram.org/bots/faq#my-bot-is-hitting-limits-how-do-i-avoid-this) 提示单聊天和群组发送频率限制。本项目采用更低的每 5 秒一条，并始终禁用付费广播。

自动化覆盖完整中文卡片、UNKNOWN、缺配置禁用、凭据脱敏、WATCH/候选过滤、过期行情、实体隔离、去重与冷却、风险优先、429 全局冷却、重试、超时 UNCERTAIN、数据库重开恢复、过期处理，以及本地 HTTP mock 的真实 Bot API 请求/响应解析。mock 使用虚构测试凭据并强制绕过代理，只绑定 loopback；它不访问真实 Telegram，不作为自然行情或真实投递的验证证据。

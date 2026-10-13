# Codex 总任务｜ChainFlow Intelligence（Rust · SOL/BSC · 零付费）

你现在是本项目的主程、测试工程师和开源发布负责人。请直接在我的 Windows 本机执行真实开发、集成测试、验证和 GitHub 发布，不要只给建议/架构/伪代码，不要结束在空壳或演示数据页面上。

## 一、最终目标与不可妥协约束

产品名：ChainFlow Intelligence；GitHub 目标账户：`Callmecx`；新仓库目标：`Callmecx/chainflow-intelligence`；建议工作目录：`D:\ChainFlow-Intelligence`（不存在才创建）。最终交付给真实交易者使用的 SOL/BSC 代币发现、跟踪、风险预警与研究看板，同时成为可验收的 Rust 工程作品。

1. 必须完全免费：不得采购/开通付费 API、节点、服务器、数据库、模型或托管。不得使用需要先输入信用卡且可能自动计费的方案。GitHub Pages 仅作为非商业开源项目展示和定时快照体验页，不能谎称托管 24/7 Rust 后端。
2. Rust/Tokio/reqwest/axum/SQLite 为唯一核心采集分析后端；Vue3+TypeScript+Vite 为前端；单 Rust 进程、单 SQLite 数据库、单写入队列。不要堆微服务、Kafka、ClickHouse、Kubernetes，不做复杂的 Gate/Authority/Writer 系统；这些未来再择需扩展。
3. **默认只读，不连接钱包、不索要私钥、不自动买卖、不实盘下单。** 只有公开行情和公共链状态。Telegram 可选且默认关闭。
4. 不得伪造数据、K线、钱包行为、成交额、测试结果、已部署状态、GitHub 链接。真实数据不可用时明确 `UNAVAILABLE / STALE / SOURCE_ERROR`，不要静默换成样例值。模拟数据要清楚标明 DEMO。
5. 尊重公开API条款、限流、授权和开源许可证。禁止破解反爬、绕过限流、爬登录态、盗用无权限API、使用来路不明的付费接口镜像。
6. 用户预算为0，优先交付最小但确实可用的 MVP，再扩功能。不要为了图表颜值推迟可运行产品，不要擅自改动其他项目及已有仓库。

## 二、先核查并写明数据源能力

实现可替换的 Provider trait 与服务状态页：

- DEX Screener 官方 API：`https://docs.dexscreener.com/api/reference`；使用 `token-profiles/latest/v1`（发现是平台收录/推广视角，不能视为全部新币）、`token-boosts/latest/v1`（明确标注 BOOSTED，不能伪装有机热度）、`token-pairs/v1/{chainId}/{tokenAddress}`、`tokens/v1/{chainId}/{tokenAddresses}`（单次最高30个）、`latest/dex/search`、`latest/dex/pairs`。资料显示发现类接口60/min、token/pair类300/min；程序默认更保守，如发现≤8/min、行情≤30/min，支持缓存、分批、指数退避与Retry-After。
- GeckoTerminal 官方公开 API V2：`https://api.geckoterminal.com/docs/index.html`，公开额度约10 calls/min，缓存约1分钟。使用 `/networks/solana/new_pools`、`/networks/bsc/new_pools`、trending_pools、pool OHLCV、pool trades 等实际文档可用端点；默认总≤6/min。先核对 network ID 和真实返回结构，再接入；不可写死未验证字段。
- Solana公共RPC：`https://api.mainnet.solana.com`，只适量做指定 mint / pool / wallet 的链上验证、订阅；公共端点官方不推荐用于生产高频全链捕获。不能依此承诺全网秒级聪明钱覆盖。Rust参考 Anza SDK：https://github.com/anza-xyz/solana-sdk ；Solana pubsub client。对所有解析保留真实签名/slot/确认级别。
- BSC官方RPC：`https://bsc-dataseed.bnbchain.org`；官方明确这些免费主网端点禁用 `eth_getLogs`。先做 eth_call / 读链数据可用性测试；全链 event logs 和连续 swap 流只能标记 UNSUPPORTED，除非经过真实验证找到了合法免费来源。EVM Rust组件参考 https://github.com/alloy-rs/alloy 。
- 额外GitHub项目做审计后才考虑依赖：https://github.com/dineshpinto/geckoterminal-rs （MIT Rust SDK，可核对维护与API版本）；https://github.com/solsonar/solana-multi-dex-decoder （MIT，JavaScript参考，需测试，不能未经核验就当Rust生产解码器）。Raydium SDK V2 为GPL-3.0，若项目选MIT开源不可直接复制其GPL实现代码；保留来源/许可证清单。
- 免费产品不强依赖 X/Twitter 官方搜索（免费权限不保证）、付费OKX API、Birdeye付费接口。叙事只能显示已明确获取的公开URL/用户手动备注，不能AI臆造。

写出 `docs/DATA_SOURCES.md` 与 `docs/LIMITATIONS.md`，记录各源API、时间戳、限流、可获得字段、缺失字段、授权与风险。Provider接口须支持 fixture/offline 回放。

## 三、真正能用的交易者产品 MVP

1. **双链发现**：SOL/BSC独立切换；综合 GeckoTerminal 的 new_pools/trending_pools 和 DEX Screener token profiles/boosts；必须给数据来源与发现偏差说明，绝不自称捕获全部新币。数据实体以 `(chain, token_address, pool_address)` 唯一键关联，多池聚合不得错把某个池FDV当全局市值。
2. **代币精确搜索**：输入Solana完整mint或BSC完整`0x`合约地址；显示名称、链、token/pool地址一键复制、价格、流动性、24h量、5m/1h交易次数、来源、获取时间、DEX平台、浏览器查证链接。MarketCap和FDV必须分字段、缺失显示未知而非0。
3. **观察池**：NEW / ACTIVE / REVIVAL_WATCH / RANGE_WATCH / MOMENTUM_WATCH / RISK / EXPIRED 等可解释标签。这些是规则观察，不是投资推荐；默认过滤例：发行/池年龄≤4h、市值（如果已知）5K-200K USD、流动性≥3K USD；另有100K-1M、1M-2M扩展视图。所有阈值可调整，不把我的历史策略视为已验证的优势策略。
4. **时序观察**：真实抓取后的价格/流动性/交易笔数快照写SQLite，5m/15m及1h结构观察仅在有足量采样时计算；另可使用Gecko官方OHLCV但标明缓存/来源。对取样K线标`SAMPLED`，不能冒充逐笔交易重建的完整K线。
5. **风险优先**：流动性相对前样本急跌、短时大回撤、买卖笔数失衡（不得直接称净资金流）、突增量后回落、数据源失联、同名代币混淆、代币地址不合法等。若无法独立验证 honeypot/top10/聪明钱钱包标签，就显示 UNKNOWN，不输出“安全”“庄家吸筹”“必涨”等肯定结论。
6. **预警状态机**：DISCOVERED -> WATCH -> CONFIRMATION_CANDIDATE -> CONFIRMED（可选）-> INVALIDATED / EXPIRED；保留首次观察、每步触发因素、证据、时间与失效条件；默认观察TTL 2小时，首次6分钟不发强确认提醒，普通风险提醒不受此延迟影响；分离“高风险”“关注”“信号确认”。同token同规则冷却和去重。无数据不能升级状态。
7. **解释与结果回测**：每条卡片列出 `WHY / EVIDENCE / INCOMPLETE_DATA / INVALIDATION`、数据源、观察UTC时间；统计触发后1h/2h/6h的最大正向/负向变化（只对后续真实采样和可核验区间；数据不全标 unknown，绝不编造或填补未来信息），对比简单基线和随机候选作为研究，而非宣称稳定盈利。
8. **客户端**：中文优先；完整Mint/合约一键复制、筛选排序、关注列表、详情走势图、来源状态、信号时间线、近期预警、响应式手机UI、深色交易终端风格；功能先于视觉；不出现空按钮和虚构数字。
9. **API**：`GET /api/health`、`/api/sources`、`/api/tokens`、`/api/tokens/{chain}/{address}`、`/api/signals`、`/api/metrics`；本地允许 `POST /api/watchlist` 等写操作但默认只监听127.0.0.1并做好输入校验。公网部署不暴露本地SQLite与写接口。
10. **Telegram（可选）**：只在配置后开启；用户手动在本机 `.env` 或GitHub Secrets中填BOT_TOKEN/CHAT_ID，任何日志/截图/网页/仓库不得泄露密钥；限频、去重、中文说明与止损风险标记，不代替用户决策。不要求我现在提供凭证即可完成绝大多数功能。
11. **可靠性**：共享限流token bucket、429/503退避、超时、断网恢复、幂等、单writer + SQLite WAL、按时滚动归档、时钟与单位一致、数据源健康状态和数据延迟分层、无静默降级、结构化日志、内存和请求计数。

## 四、工程结构和实现步骤

建议：
```
chainflow-intelligence/
  backend/Cargo.toml
  backend/src/{main.rs,config.rs,providers/,models/,storage/,engine/,alerts/,api/}
  frontend/{package.json,src/,vite.config.ts}
  data/demo.json
  scripts/windows/{doctor.ps1,start.ps1,stop.ps1}
  docs/{ARCHITECTURE,DATA_SOURCES,LIMITATIONS,SIGNAL_RULES,DEPLOYMENT,RUNBOOK,ACCEPTANCE,LICENSE_AUDIT}.md
  .github/workflows/{ci.yml,pages.yml}
  .env.example
  .gitignore
  README.md
  LICENSE
```

阶段A：核验Windows环境（Rust、Node LTS、Git、GitHub CLI等）和账户登录；缺工具可使用合法免费的官方安装途径，不能在未经我同意时做高风险全局系统更改。检查目标目录是否已存在仓库；存在时只安全增量更新。检查 `D:\V23_WORKTREES` 与 `D:\V23_OPERATIONS` 是否存在，仅在存在时只读参考接口/规则，**禁止覆盖旧系统、拷贝本机隐私文件、迁移私钥**。

阶段B：打通真实Provider、`scan-once`命令、同链同合约去重、SQLite写入、CLI健康检查、数据验证、单元/集成fixture测试；在任何Web美化前给出终端中至少几个经过真实API抓取且附地址/来源/时间的例子。若外部环境无法访问互联网，真实验证明确写 blocked 并保留可回放测试，不得宣称在线通过。

阶段C：实现规则与风控、REST API和前端；本机执行时前端从真实本地API读取。为GitHub Pages实现公开快照模式（下一节），两种模式在页面清楚区分，避免误导。

阶段D：实现Windows一键启动/关闭/诊断、异常恢复、中文README；CI跑`cargo fmt --check`、`cargo clippy`、`cargo test`、前端类型检查/构建/测试，尽可能加Playwright移动端基本验证。用实际结果写 `docs/ACCEPTANCE.md`。

## 五、零付费GitHub发布架构（必须实现并标明时效）

1. GitHub `Callmecx/chainflow-intelligence` **公开仓库**托管全部源码、文档、MIT（仅在依赖许可证兼容时）、CI、启动脚本；前端部署为**非商业公开演示/研究看板**到 GitHub Pages。GitHub Pages是静态站点不能后台运行Rust，且其服务条款不支持将其作为商业SaaS托管，所以不要写成“24小时实时服务”。
2. 核心Rust CLI必须有`scan-once --output <path>`，可在GitHub Actions标准Linux runner做简短真实扫描并生成 `snapshot.json`，字段至少包括`generated_at_utc`、`sources`、`items`、`max_staleness_seconds`、`provider_errors`、`mode=PUBLIC_SNAPSHOT`，严禁输出密钥。**前端只把此次真实扫描结果标为快照**；若失败显示“数据更新失败/无最新数据”，DEMO数据在显著位置独立标记。
3. `.github/workflows/pages.yml`在push、workflow_dispatch和UTC每半小时的第17、47分运行（如`17,47 * * * *`）；同一job中构建Rust scan-once、生成最新snapshot并构建Vue，使用官方GitHub Pages上传/部署Actions，不做每半小时一次自动Git提交，也不把snapshot写入git历史。GitHub Actions可能延迟/掉队；首页显示实际`generated_at`，超过配置的阈值明显显示 `STALE`。
4. 前端 Vite 配置 `/chainflow-intelligence/` base路径并兼容刷新/深链接；构建包含无敏感内容的静态资产。在线版搜索范围受当次快照限制；**不得让用户误以为自选代币会被公网服务器实时监控**。个人自选代币与连续推送功能需启动本机Rust服务。
5. GitHub Actions在公开仓库标准runner通常免费，但依赖调度机制且无严格SLA；文档明确“半小时目标快照，不是连续采集”。若Pages权限或Action失败，清晰告知我需在GitHub哪个设置页手动启用，不得假报已部署。
6. 默认无Cloudflare或其他云账户要求，若以后另行授权可扩为Cloudflare Workers免费额度内的轻量服务；本轮不为追求所谓24/7而引入额外平台、信用卡或付费依赖。

## 六、测试验收：不能只报告PASS，要有可核验证据

- 真实连接：至少验证Solana和BSC各有实际代币/池快照（若其中某源无数据需区分无覆盖/失败），5个完整地址的来源可交叉打开，对市值/FDV/池地址核对。
- 可回放单测：相同contract跨源去重，错误链ID，缺失市值字段，除零/极小流动性，时间顺序乱序，429/timeout/Retry-After/403，WebSocket中断（若启用），SQLite重启幂等，重复预警与冷却，归档与恢复。
- 本机运行：真实来源可用的前提下至少做一个15—30分钟Canary，输出运行起止、处理条数、调用次数、告警数、异常数、数据库完整性与每源更新延迟。无法达到的项写明原因，不得补造统计。
- 站点检查：真实打开GitHub Pages并检查首页和至少1个token详情、移动视图、完整地址复制按钮、外链、源数据时间、无数据/陈旧模式。
- 文档至少回答：①如何免费在Windows启动；②哪些来自真实公开API；③平台实际上不能监控什么；④为何Github Pages不能替代后台；⑤如何关闭、查日志、备份sqlite；⑥如何撤销或更换Telegram凭证。

## 七、GitHub发布与权限交接

在本机执行`gh auth status`，如果未登录，暂停让我人工进行`gh auth login`，不要索要密码、PAT或私钥，不要未经授权把任何本地`.env`、SQLite、用户交易记录或密钥上传。若`Callmecx/chainflow-intelligence`已存在，先拉取比较并安全处理，禁止覆盖/force-push；若不存在，先本地完成最小可用版本和测试，再用`gh repo create Callmecx/chainflow-intelligence --public --source . --remote origin --push`或等价受支持命令创建并推送。授权已包含在我让你执行本任务的要求里，但遇到额外登录验证、条款确认或敏感凭证时让我接管。

如果已有合适权限，执行设置GitHub Pages Actions发布；如无法由CLI启用，精确告诉我需要点击的页面。**必须检查远端Git仓库的最新commit SHA、CI状态、Pages实际URL以及HTTP响应**，不能把“计划URL”当已成功上线。不要在未证实发布时声称部署完成。

## 八、最终汇报格式

1. GitHub仓库真实链接、commit SHA、Actions最新状态、Pages实际可访问链接（若失败写Blocked）。
2. Windows本机启动/停止/诊断命令；哪个功能仅本地、哪个功能网页可用。
3. 各免费Provider实际连通与限流表；样本地址、抓取时间和来源链接。
4. 已完成/未完成模块清单，明确不能实现的部分及原因；本轮是否产生任何费用（必须0）。
5. 测试与Canary真实结果、仍存在的故障、下一轮优先级建议。

**执行要求：不要只写方案。现在开始审查环境、建仓、落地代码、真实联调、测试、上传GitHub并验证。每完成一个里程碑就提交commit。遇到阻断先做能做的，需我登录/确认时再精准询问。优先保证免费、可用、可验证，不搞大而空。**

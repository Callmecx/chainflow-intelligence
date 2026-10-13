# 免费数据源、字段和实测证据

核验日期：2026-10-13；所有请求只读取公开数据，没有 API key、付费套餐或交易签名。时间存储为 UTC，金额单位为 USD。动态行情可能在打开链接时已变化；下面的历史值仅用于连接与字段核验。

## 免费来源与实际能力

| 来源 | 官方文档与接口 | 程序限流与缓存 | 实测能力、边界 |
|---|---|---|---|
| DEX Screener | [API reference](https://docs.dexscreener.com/api/reference)：`token-profiles/latest/v1`、`token-boosts/latest/v1`、`tokens/v1/{chain}/{addresses}`、`token-pairs/v1/{chain}/{address}` | 发现共用 8/min；行情共用 30/min；容量 1 的共享令牌桶；发现缓存 45 秒，行情缓存 20 秒 | SOL/BSC 真实价格、池流动性、成交量、买卖笔数；marketCap 与 fdv 独立且可缺失。profiles 是平台收录，boosts 是推广流量，明确标记 PROFILE / BOOSTED；不能解释为全部新币或有机热度。 |
| GeckoTerminal V2 | [官方 OpenAPI](https://api.geckoterminal.com/docs/index.html)：network ID 已验证为 `solana` / `bsc`；`new_pools`、`trending_pools`、token pools、pool OHLCV / trades | 所有 Gecko 调用共用 6/min；容量 1；缓存 60 秒；Accept 指定 `application/json;version=20230203` | 两链 new/trending 均 HTTP 200。返回 JSON:API 的 data / included / relationships；base_token 的地址从关系提取，不把池地址当代币。公开 API 仍属 beta，端点覆盖随索引情况变化。 |
| Solana 官方公共 RPC | [公共节点说明](https://solana.com/docs/references/clusters)；`https://api.mainnet.solana.com` | RPC 共用约 20/min；缓存 60 秒；当前每轮只查询一个基准 mint | `getAccountInfo` 对 WSOL 成功；保存 slot 与 `confirmed`。只证明该指定账户在响应时可读，不能证明任意币安全或全网交易完整。未启用 WebSocket、全链索引或钱包标签。 |
| BSC 官方公共 RPC | [官方端点限制](https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/json-rpc-endpoint/)；`https://bsc-dataseed.bnbchain.org` | 一个 batch 含 eth_chainId / eth_blockNumber / eth_call；RPC 共用桶；缓存 60 秒 | 真实返回 chainId `0x38`；WBNB `decimals()` 为 18。官方免费端点文档禁用 eth_getLogs，状态页明确列为 UNSUPPORTED；没有连续全链 swap 流。 |

Gecko 的官方 API 文档写约 10 calls/min，CoinGecko 帮助页曾写 30 calls/min，存在公开文档口径差异；本实现始终采用更低的 6/min，不依赖较高数字。DEX 文档发现/行情上限分别为 60/min 与 300/min，本实现更保守。限流是同一个 `ProviderHub` 进程生命周期内共享，跨 scan / lookup / OHLCV / trades 复用；重启和另一个进程不共享令牌，因此只建议运行单个本地实例。

## 扫描预算与持续观察

每轮采集 Gecko 两链各一页 new/trending，每页最多保留 12 个池；DEX profiles/boosts 每类每链最多加入 8 个地址。每链按优先级最多 90 个代币，30 地址一批，最多 3 批；每 token 最多 4 个流动性较高的池。调用方传入个人关注和近期发现的优先顺序被保留，超过预算会写 `provider_errors`，不会谎报完整覆盖。基准 WSOL/WBNB 标记 `BASELINE`，用于连接对照，不能把它们视作新币或策略信号。

`latest/dex/search` 与 `latest/dex/pairs` 已提供 Provider 读取方法，精确搜索界面通过 token-pairs 与 Gecko token pools 查询。Gecko OHLCV / recent trades 提供读取方法并共享限流，但本轮 UI 使用 SQLite 实际抓取后的 `SAMPLED` 观察曲线；没有把抽样价格伪装为逐笔 K 线。recent trades 的原始链上哈希字段随响应保留，但不作为全网成交覆盖承诺。

任何一个独立端点失败都会保留错误，并继续其他端点。HTTP 429 / 503 支持指数退避和 Retry-After（秒或 HTTP 日期）；长冷却保留在共享桶中并提前返回，避免一次扫描被挂住。403 不重试，网络故障/超时最多 3 次。连接超时 5 秒、每次请求 10 秒。缓存返回时保留原抓取时间，过期缓存不会伪装成最新数据。请求计数是实际 HTTP 尝试次数，缓存命中不增加计数，重试失败会计入错误。

## 字段语义

- 唯一实体是 `(chain, token_address, pool_address)`，BSC 地址小写归一化，Solana 区分大小写。跨源同实体选择较新的完整样本，合并发现标签；不相加 FDV / marketCap，不把一个池 FDV 当全局市值。
- `price_usd` 是 base token 价格；查询返回 quote token 时不擅自倒数或挪用 priceUsd，而是只保留确切 base token 匹配。
- `liquidity_usd` 分别取 DEX liquidity.usd 与 Gecko reserve_in_usd；`volume_24h_usd` 是该池的窗口成交量。买卖笔数不是资金流或独立钱包数。
- `market_cap_usd` 与 `fdv_usd` 分字段读取。null/缺失/无法解析保留 unknown；第三方原始字段即便已存在也不表示本项目完成了独立流通供应验证。
- `pool_created_at` 是源报告的池创建时间，不是链上代币发行时间。`fetched_at` 是本机 HTTP 接收时间，不是链上成交时间；聚合器和官方缓存仍会增加延迟。
- 没有独立验证的 honeypot、top10、聪明钱、叙事和资金控盘标签均不可据此输出确定结论。RPC 基准检查不覆盖用户输入的所有合约。

## 真实连接样本

机器可读记录在 [provider-validation.json](evidence/provider-validation.json)、[provider-rpc-probe.json](evidence/provider-rpc-probe.json) 和 [provider-additional-validation.json](evidence/provider-additional-validation.json)。原始完整大响应仅保留在本地 `data/provider-probes/`，不作为运行时回退。测试中两个小型公开响应片段位于 `backend/src/providers/fixtures/`，只用于显式离线回放。

| 链 / 抓取 UTC | 完整 token 地址 | 完整 pool 地址与来源查证 |
|---|---|---|
| SOL / 2026-10-13 07:55:43Z | `So11111111111111111111111111111111111111112` | `Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE` · [DEX 池](https://dexscreener.com/solana/Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE) |
| BSC / 2026-10-13 07:55:44Z | `0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c` | `0x172fcD41E0913e95784454622d1c3724f546f849` · [DEX 池](https://dexscreener.com/bsc/0x172fcD41E0913e95784454622d1c3724f546f849) |
| SOL / 2026-10-13 07:55:44Z | `9t1i1aZ7nMPJ2zav6haP8BDeqAnLdqYXFEsBbTzZpump` | `35FbS7b8FAucw5qUPvspbV1bWTnWPM8x7ad86S52UBB7` · [Gecko 池](https://www.geckoterminal.com/solana/pools/35FbS7b8FAucw5qUPvspbV1bWTnWPM8x7ad86S52UBB7) |
| SOL / 2026-10-13 07:55:44Z | `ERDiDmqoLVvkeC6u3F34ZGaX4fxDs1tFtHXrm1U4pump` | `AgKf1sMpXGyFwzmFbaXa6R6TB4JcUtyfhkc91BU6gHkc` · [Gecko 池](https://www.geckoterminal.com/solana/pools/AgKf1sMpXGyFwzmFbaXa6R6TB4JcUtyfhkc91BU6gHkc) |
| BSC / 2026-10-13 07:56:35Z | `0x2b3aa3b34425e96a4c3dda4f914489d3500effff` | `0x2b3aa3b34425e96a4c3dda4f914489d3500effff` · [Gecko 池](https://www.geckoterminal.com/bsc/pools/0x2b3aa3b34425e96a4c3dda4f914489d3500effff) |
| SOL / 2026-10-13 07:56:46Z | `HMYd9tosnUXuNHmq7pXmoePRVBLBBjA3JBfydq6upump` | `12jc1DzJpzbCDaKuM4brAKh9jcFry2TLG1FfGsL4ftCG` · [Gecko 池](https://www.geckoterminal.com/solana/pools/12jc1DzJpzbCDaKuM4brAKh9jcFry2TLG1FfGsL4ftCG) |

该 BSC new-pool 样本的 provider 将 token/pool 地址报告为相同值，保留其原始关系，不假定它是标准 V2 Pair。部分 V4 池使用 bytes32 ID；页面与采集保留原始池 ID，当前可选按池查询方法只支持标准地址。

字段交叉核对：DEX WBNB 第一个池的 marketCap 和 fdv 都为 `1157488689`，而 WSOL 响应缺失二者；Gecko `9t1i…pump` 样本 market_cap 为 null、fdv 为 `3147.772942`，程序保持差别。真实 RPC 检查中 Solana slot 为 `456367578`（confirmed），BSC block 为 `0x79775e7`（latest，不能据此声称 finalized）。

## 授权与成本

只使用公开官方 API；无登录抓取、镜像绕限、付费代理或信用卡。遵守速率、缓存和公开 API 文档，保留数据归属与来源链接。API 的可公开读取不意味着数据版权转移；本仓库 MIT 只覆盖自有代码，不授予第三方行情数据的额外许可。公开页面用于非商业研究展示，源条款和可用性仍可能变化；若源限制变更应停用该源而不是绕过。源失联会显示错误/陈旧数据，不补造价格或测试结果。

## Windows 代理差异诊断

首次 Rust 实际扫描曾出现 DEX / Gecko / Solana 超时，而同机 PowerShell 请求成功、BSC 可直连。实际核对锁定 `reqwest 0.12.28` 源码发现：`default-features=false` 会关闭默认包含的 `system-proxy`，但 `rustls-tls` 不会重新开启该特性。PowerShell 与 Rust 对 Windows 系统代理的自动读取因此不同；成功的 PowerShell 验证不能替代 Rust 进程的连接验收。

代码现已保持 `default-features=false` 并在 reqwest 特性中加入 `system-proxy`；它启用 `hyper-util/client-proxy-system`，使 Windows 静态代理的 ProxyEnable / ProxyServer / ProxyOverride 可被读取。仍使用 rustls 和正常证书校验。mock HTTP 测试只在测试辅助 client 上使用 `.no_proxy()`，避免开发者代理影响离线回放；生产 client 保持系统代理支持。实际 Rust 网络扫描结果以验收记录为准。

不重建的当前会话排障办法是设置已有正常代理的 `HTTPS_PROXY` / `HTTP_PROXY`；这些环境变量在上述 reqwest 配置下也会被读取。只设置当前进程和其子进程，保留用户已有显式配置，`NO_PROXY` 应包含 `localhost,127.0.0.1,::1`。不要把代理凭据或个人系统代理配置写入仓库、快照和日志。这里只复用用户本机已经设置的网络代理，不购买服务、不更换 API 地址、不绕过数据源限流。

底层 hyper-util 的 Windows 自动实现读取静态注册表设置；PAC/WPAD 自动脚本不是该实现的完整能力，企业网络仍可能需要显式环境变量。代理变化后须重启 Rust 进程以重新构建 HTTP client。官方说明可见 [reqwest proxies/features](https://docs.rs/reqwest/0.12.28/reqwest/)，具体锁定源码路径与核对结论见 [代理证据](evidence/provider-proxy-diagnosis.json)。

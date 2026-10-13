# 内部接口契约

时间为 RFC3339 UTC，chain 仅 solana / bsc。金额 USD 为 number|null，未知不得置0。

Rust models.rs 由主代理创建。Provider 使用 `crate::models::{TokenSnapshot,SourceStatus}`。

TokenSnapshot 字段：chain:String, token_address:String, pool_address:String, name:String, symbol:String, dex:String, price_usd:Option<f64>, liquidity_usd:Option<f64>, volume_24h_usd:Option<f64>, market_cap_usd:Option<f64>, fdv_usd:Option<f64>, buys_5m:Option<u64>, sells_5m:Option<u64>, buys_1h:Option<u64>, sells_1h:Option<u64>, price_change_5m_pct:Option<f64>, price_change_1h_pct:Option<f64>, pool_created_at:Option<String>, fetched_at:String, source:String, source_url:String, discovery_labels:Vec<String>。

SourceStatus 字段：id:String, status:String (OK/SOURCE_ERROR/UNAVAILABLE/UNSUPPORTED), checked_at:String, last_success_at:Option<String>, latency_ms:Option<u64>, requests:u64, errors:u64, detail:String。

ProviderResult 字段：items:Vec<TokenSnapshot>, sources:Vec<SourceStatus>, provider_errors:Vec<String>；providers/mod.rs 定义。公开 `ProviderHub::new() -> anyhow::Result<Self>`，`async fn scan(&self, watchlist:&[(String,String)]) -> ProviderResult`，`async fn lookup(&self, chain:&str,address:&str) -> ProviderResult`。限流状态由 Hub 持久共享。Provider trait 必须支持 fixture 回放。若添加方法与主代理协商。

接口快照 Snapshot：mode:LOCAL_LIVE|PUBLIC_SNAPSHOT, generated_at_utc:String, max_staleness_seconds:u64, sources:SourceStatus[], items:TokenView[], provider_errors:string[], signals:Signal[], metrics:object。

TokenView：平铺上述 TokenSnapshot 字段，额外 label:String, state:String, first_seen_at:String, why:string[], evidence:string[], incomplete_data:string[], invalidation:string[], history:Point[], timeline:Signal[], outcome:object。Point:{at:string,price_usd:number|null,liquidity_usd:number|null,volume_24h_usd:number|null}。

Signal：id:string, chain:string, token_address:string, pool_address:string, rule:string, severity:RISK|WATCH|CONFIRMATION, at:string, state:string, message:string, evidence:string[]。

GET /api/tokens -> Snapshot；GET /api/tokens/{chain}/{address} -> TokenView[]（未收录时真实查询）；GET /api/sources -> SourceStatus[]；GET /api/signals -> Signal[]；GET /api/metrics -> object；GET /api/health -> object。GET /api/watchlist -> {chain,address}[]；POST /api/watchlist body {chain,address} -> 200；DELETE /api/watchlist/{chain}/{address} -> 200。

前端：Vite base /chainflow-intelligence/；使用 hash token 详情；VITE_MODE=PUBLIC_SNAPSHOT 默认 public/snapshot.json，LOCAL_LIVE 通过 Vite /api 代理 127.0.0.1:8787。在线本地收藏仅浏览器，明确不触发服务监控。

CLI: cargo run --manifest-path backend/Cargo.toml -- scan-once --output data/snapshot.json [--db data/chainflow.sqlite]；serve [--db ...] [--port 8787] [--interval-seconds 120]；doctor --db ...；archive --db ... --output data/archive.json --retention-days 30；restore --db ... --input ...。

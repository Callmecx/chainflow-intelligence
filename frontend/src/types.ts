export interface SourceStatus {
  id: string; status: string; checked_at: string; last_success_at: string | null
  latency_ms: number | null; requests: number; errors: number; detail: string
}
export interface Signal {
  id: string; chain: string; token_address: string; pool_address: string
  rule: string; severity: 'RISK' | 'WATCH' | 'CONFIRMATION'; at: string
  state: string; message: string; evidence: string[]
}
export interface Point { at: string; price_usd: number | null; liquidity_usd: number | null; volume_24h_usd: number | null }
export interface TokenView {
  chain: string; token_address: string; pool_address: string; name: string; symbol: string; dex: string
  price_usd: number | null; liquidity_usd: number | null; volume_24h_usd: number | null
  market_cap_usd: number | null; fdv_usd: number | null
  buys_5m: number | null; sells_5m: number | null; buys_1h: number | null; sells_1h: number | null
  price_change_5m_pct: number | null; price_change_1h_pct: number | null
  pool_created_at: string | null; fetched_at: string; source: string; source_url: string
  discovery_labels: string[]; label: string; state: string; first_seen_at: string
  why: string[]; evidence: string[]; incomplete_data: string[]; invalidation: string[]
  history: Point[]; timeline: Signal[]; outcome: Record<string, unknown>
}
export interface Snapshot {
  mode: 'LOCAL_LIVE' | 'PUBLIC_SNAPSHOT'; generated_at_utc: string
  max_staleness_seconds: number; sources: SourceStatus[]; items: TokenView[]
  provider_errors: string[]; signals: Signal[]; metrics: Record<string, unknown>
}
export interface WatchItem { chain: string; address: string }

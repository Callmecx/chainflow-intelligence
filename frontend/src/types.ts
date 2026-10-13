export interface SourceStatus {
  id: string; status: string; checked_at: string; last_success_at: string | null
  latency_ms: number | null; requests: number; errors: number; detail: string
}
export interface Signal {
  id: string; chain: string; token_address: string; pool_address: string
  rule: string; severity: 'RISK' | 'WATCH' | 'CONFIRMATION' | 'DATA_FAULT'; at: string
  state: string; message: string; evidence: string[]
  category?: 'MARKET_RISK' | 'DATA_FAULT' | 'OBSERVATION' | 'CONFIRMATION'
  rule_version?: string; source_at?: string | null
  baseline_price_usd?: number | null; baseline_liquidity_usd?: number | null
  factors?: SignalFactor[]; evidence_snapshot?: Record<string, unknown>
}
export interface SignalFactor { name: string; value: number | string | null; weight: number | null; score: number | null; evidence: string[] }
export type ResearchWindow = '1h' | '2h' | '6h'
export interface SignalOutcome {
  status: string; samples: number; return_pct: number | null
  max_positive_pct: number | null; max_drawdown_pct: number | null; liquidity_change_pct: number | null
  invalidated: boolean | null; target_at: string | null; observed_at: string | null
  max_gap_seconds: number | null; reason?: string
  tolerance_seconds?: number | null; source_changed?: boolean
}
export interface SignalResearch {
  signal_id: string; chain: string; token_address: string; pool_address: string
  rule: string; rule_version: string; trigger_at: string; source_at: string | null
  baseline_price_usd: number | null; baseline_liquidity_usd: number | null
  evidence_snapshot: Record<string, unknown>; factors: SignalFactor[]
  windows: Partial<Record<ResearchWindow, SignalOutcome>>
}
export interface RuleStat {
  rule: string; rule_version: string; window: ResearchWindow; status: string
  trigger_count: number; eligible_count: number; valid_count: number; missing_count: number
  censored_count: number; pending_count: number; missing_rate: number | null
  mean_return_pct: number | null; median_return_pct: number | null
  distribution: { negative: number; flat: number; positive: number }
}
export interface ResearchReport {
  generated_at_utc: string; status: string; min_sample_size: number
  records: SignalResearch[]; rule_stats: RuleStat[]; limitations: string[]
}
export interface SourceHealth {
  id: string; consecutive_failures: number; recoveries: number
  last_checked_at: string | null; last_success_at: string | null; source_age_seconds: number | null
}
export interface LocalHealth {
  mode: 'LOCAL_LIVE'; status: string; started_at: string; last_scan_at: string | null
  database?: { integrity_check?: string; journal_mode?: string; samples?: number; [key: string]: unknown }
  runtime?: { scans?: number; accepted_samples?: number; provider_errors?: number; last_scan_duration_ms?: number; memory_rss_bytes?: number | null; [key: string]: unknown }
  telegram?: Record<string, unknown>
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
  research?: ResearchReport | null
}
export interface WatchItem { chain: string; address: string }

import type { LocalHealth, ResearchReport, SignalOutcome, Snapshot, TokenView } from '../src/types'

// AUTOMATED TEST FIXTURE ONLY. Never copied into public/ or a production build.
export const solAddress = 'So11111111111111111111111111111111111111112'
export const bscAddress = '0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c'
export function fixtureToken(overrides: Partial<TokenView> = {}): TokenView {
  const now = Date.now()
  return {
    chain: 'solana', token_address: solAddress, pool_address: 'TEST_FIXTURE_POOL_SOL',
    name: '自动化测试夹具 · 非真实行情', symbol: 'TEST_SOL', dex: 'TEST_FIXTURE_DEX',
    price_usd: 1, liquidity_usd: 15000, volume_24h_usd: 5000, market_cap_usd: null, fdv_usd: 100000,
    buys_5m: 8, sells_5m: 5, buys_1h: 15, sells_1h: 10, price_change_5m_pct: null, price_change_1h_pct: 1.5,
    pool_created_at: new Date(now - 3600000).toISOString(), fetched_at: new Date(now).toISOString(),
    source: 'TEST_FIXTURE_ONLY', source_url: 'https://example.com/test-fixture', discovery_labels: ['TEST_FIXTURE_ONLY'],
    label: 'NEW', state: 'WATCH', first_seen_at: new Date(now - 600000).toISOString(),
    why: ['TEST_FIXTURE_ONLY · 用于验证解释呈现'], evidence: ['TEST_FIXTURE_ONLY'], incomplete_data: ['市值未知'], invalidation: ['TEST_FIXTURE_ONLY · 数据失效'],
    history: [{ at: new Date(now - 60000).toISOString(), price_usd: 0.9, liquidity_usd: 14000, volume_24h_usd: 4000 }, { at: new Date(now).toISOString(), price_usd: 1, liquidity_usd: 15000, volume_24h_usd: 5000 }],
    timeline: [], outcome: { kind: 'SAMPLED_OBSERVATION', windows: { '1h': { status: 'UNKNOWN', samples: 2, max_positive_pct: null, max_negative_pct: null, coverage_seconds: 60, reason: 'TEST_FIXTURE_ONLY · 时间窗口不完整' } } },
    ...overrides,
  }
}
export function fixtureSnapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  const now = new Date().toISOString()
  return {
    mode: 'PUBLIC_SNAPSHOT', generated_at_utc: now, max_staleness_seconds: 3600,
    sources: [{ id: 'TEST_FIXTURE_PROVIDER', status: 'OK', checked_at: now, last_success_at: now, latency_ms: 5, requests: 1, errors: 0, detail: '自动化测试夹具，非真实来源验证' }],
    items: [fixtureToken(), fixtureToken({ chain: 'bsc', token_address: bscAddress, pool_address: 'TEST_FIXTURE_POOL_BSC', symbol: 'TEST_BSC', label: 'RISK', market_cap_usd: 125000, price_change_1h_pct: -15 })],
    provider_errors: [], signals: [{ id: 'TEST_FIXTURE_SIGNAL', chain: 'bsc', token_address: bscAddress, pool_address: 'TEST_FIXTURE_POOL_BSC', rule: 'TEST_FIXTURE_RISK', severity: 'RISK', at: now, state: 'INVALIDATED', message: '自动化测试风险事件', evidence: ['TEST_FIXTURE_ONLY'] }], metrics: {}, ...overrides,
  }
}

export function fixtureResearch(): ResearchReport {
  const now = new Date().toISOString()
  const complete: SignalOutcome = {status:'COMPLETE',samples:31,return_pct:5,max_positive_pct:8,max_drawdown_pct:-3,liquidity_change_pct:2,invalidated:false,target_at:now,observed_at:now,max_gap_seconds:120,reason:'TEST_FIXTURE_ONLY · 明确标识的测试窗口'}
  return {
    generated_at_utc:now,status:'INSUFFICIENT_SAMPLE',min_sample_size:30,
    records:[{signal_id:'TEST_RESEARCH_SIGNAL',chain:'solana',token_address:solAddress,pool_address:'TEST_FIXTURE_POOL_SOL',rule:'TEST_BREAKOUT',rule_version:'TEST_v1.1',trigger_at:now,source_at:now,baseline_price_usd:1,baseline_liquidity_usd:15000,evidence_snapshot:{symbol:'TEST_SOL',price_usd:1,source:'TEST_FIXTURE_ONLY'},factors:[{name:'TEST_FACTOR',value:1.2,weight:2,score:2,evidence:['TEST_FIXTURE_ONLY']}],windows:{'1h':complete,'2h':{...complete,status:'MISSING',samples:2,return_pct:null,max_positive_pct:null,max_drawdown_pct:null,liquidity_change_pct:null,observed_at:null,reason:'TEST_FIXTURE_ONLY · 目标时点缺失'},'6h':{...complete,status:'PENDING',samples:0,return_pct:null,max_positive_pct:null,max_drawdown_pct:null,liquidity_change_pct:null,observed_at:null,reason:'TEST_FIXTURE_ONLY · 尚未到达未来窗口'}}}],
    rule_stats:['1h','2h','6h'].map(window=>({rule:'TEST_BREAKOUT',rule_version:'TEST_v1.1',window:window as '1h'|'2h'|'6h',status:'INSUFFICIENT_SAMPLE',trigger_count:1,eligible_count:window === '6h'?0:1,valid_count:window === '1h'?1:0,missing_count:window === '2h'?1:0,censored_count:0,pending_count:window === '6h'?1:0,missing_rate:window === '2h'?1:window === '1h'?0:null,mean_return_pct:window === '1h'?5:null,median_return_pct:window === '1h'?5:null,distribution:{negative:0,flat:0,positive:window === '1h'?1:0}})),
    limitations:['TEST_FIXTURE_ONLY · 采样结果不包含区间内完整价格路径'],
  }
}

export function fixtureLocalHealth(): LocalHealth {
  const now = new Date().toISOString()
  return {mode:'LOCAL_LIVE',status:'OK',started_at:now,last_scan_at:now,database:{integrity_check:'ok',journal_mode:'wal'},runtime:{scans:2,accepted_samples:4,provider_errors:0,last_scan_duration_ms:100},telegram:{configured:false,enabled:false,verification:'PENDING',queue:{PENDING:0,UNCERTAIN:1},last_delivery:{state:'UNCERTAIN',code:'TIMEOUT_REQUIRES_REVIEW',at_unix:Math.floor(Date.now()/1000)},exactly_once:false}}
}

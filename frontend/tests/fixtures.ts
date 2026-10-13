import type { Snapshot, TokenView } from '../src/types'

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

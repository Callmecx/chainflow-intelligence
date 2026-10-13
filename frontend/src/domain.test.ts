import { describe, expect, it } from 'vitest'
import { addressEquals, chartSegments, filterTokens, isAddress, isStale, money, parseHash, safeUrl, tokenHash, type Filters } from './domain'
import { fixtureToken, solAddress, bscAddress } from '../tests/fixtures'

const filters: Filters = { chain: 'all', text: '', label: 'all', minimumLiquidity: 0, minimumCap: 0, maximumCap: 0, maximumAgeHours: 0, includeUnknownCap: true, sort: 'liquidity' }
describe('data integrity and source boundaries', () => {
  it('keeps unknown market cap distinct from a known zero', () => { expect(money(null)).toBe('未知'); expect(money(undefined)).toBe('未知'); expect(money(NaN)).toBe('未知'); expect(money(0)).toBe('$0.00'); expect(money(0.0000000000001)).toBe('$1.0000e-13') })
  it('validates the selected chain and rejects partial addresses', () => { expect(isAddress('solana', solAddress)).toBe(true); expect(isAddress('bsc', bscAddress)).toBe(true); expect(isAddress('bsc', solAddress)).toBe(false); expect(isAddress('solana', bscAddress)).toBe(false); expect(isAddress('bsc', '0x123')).toBe(false) })
  it('preserves Solana case but accepts EVM address casing', () => { expect(addressEquals('solana', solAddress, solAddress.toLowerCase())).toBe(false); expect(addressEquals('bsc', bscAddress, bscAddress.toLowerCase())).toBe(true) })
  it('rejects unsupported chains and Solana keys that do not decode to 32 bytes', () => { expect(isAddress('ethereum', solAddress)).toBe(false); expect(isAddress('solana', '1'.repeat(33))).toBe(false); expect(isAddress('solana', '1'.repeat(32))).toBe(true) })
  it('does not substitute FDV when filtering unknown market cap', () => { const item = fixtureToken({ market_cap_usd: null, fdv_usd: 100000 }); expect(filterTokens([item], { ...filters, minimumCap: 5000, includeUnknownCap: false }, Date.now())).toEqual([]); expect(filterTokens([item], { ...filters, minimumCap: 5000, includeUnknownCap: true }, Date.now())).toEqual([item]) })
  it('sorts missing liquidity last and filters missing age conservatively', () => { const missing = fixtureToken({ liquidity_usd: null, pool_created_at: null }); const known = fixtureToken(); expect(filterTokens([missing, known], filters, Date.now())[0]).toBe(known); expect(filterTokens([missing], { ...filters, maximumAgeHours: 4 }, Date.now())).toEqual([]) })
  it('detects stale and invalid timestamp independently of provider status', () => { expect(isStale('2025-01-01T00:00:00Z', 100, Date.parse('2025-01-01T00:02:00Z'))).toBe(true); expect(isStale('invalid', 100, Date.now())).toBe(true); expect(isStale(new Date().toISOString(), 100, Date.now())).toBe(false) })
  it('rejects unsafe evidence links', () => { expect(safeUrl('javascript:alert(1)')).toBeUndefined(); expect(safeUrl('https://dexscreener.com/solana/a')).toBe('https://dexscreener.com/solana/a') })
  it('preserves full token and pool identities in hash links', () => { const item = fixtureToken(); expect(parseHash(tokenHash(item))).toEqual({chain:item.chain,address:item.token_address,pool:item.pool_address}); expect(parseHash('#token/unknown/a')).toBeNull(); expect(parseHash('#token/%E0%A4%A')).toBeNull() })
})
describe('honest sampled chart', () => {
  it('does not manufacture a curve from one sample or duplicate times', () => { const item = fixtureToken(); expect(chartSegments(item.history.slice(0,1))).toBeNull(); expect(chartSegments([item.history[0]!,item.history[0]!])).toBeNull() })
  it('orders out-of-order samples and breaks lines at missing values', () => {
    const point = (at: string, value: number | null) => ({ at, price_usd:value, liquidity_usd:null, volume_24h_usd:null })
    const chart = chartSegments([point('2026-01-01T00:02:00Z',2),point('2026-01-01T00:00:00Z',1),point('2026-01-01T00:01:00Z',null)])
    expect(chart?.count).toBe(2); expect(chart?.paths).toHaveLength(2); expect(chart?.start).toBe('2026-01-01T00:00:00Z'); expect(chart?.min).toBe(1); expect(chart?.max).toBe(2)
  })
  it('renders a constant series without division by zero', () => { const item = fixtureToken(); const chart = chartSegments(item.history.map(p => ({ ...p, price_usd: 0 }))); expect(chart?.paths[0]).not.toMatch(/NaN|Infinity/); expect(chart?.min).toBe(0) })
})

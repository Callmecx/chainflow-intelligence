import type { Point, TokenView } from './types'

export const keyOf = (item: Pick<TokenView, 'chain' | 'token_address' | 'pool_address'>) => `${item.chain}/${item.token_address}/${item.pool_address}`
export const addressEquals = (chain: string, a: string, b: string) => chain === 'bsc' ? a.toLowerCase() === b.toLowerCase() : a === b
export const isAddress = (chain: string, value: string) => {
  if (chain === 'bsc') return /^0x[0-9a-fA-F]{40}$/.test(value)
  if (chain !== 'solana' || !/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(value)) return false
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
  let decoded = 0n
  for (const char of value) decoded = decoded * 58n + BigInt(alphabet.indexOf(char))
  let bytes = 0
  while (decoded > 0n) { decoded >>= 8n; bytes++ }
  return bytes + (value.match(/^1*/)?.[0].length ?? 0) === 32
}
export const shortAddress = (value: string) => value.length > 16 ? `${value.slice(0, 6)}…${value.slice(-6)}` : value
export const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value)
export const money = (value: number | null | undefined, compact = true): string => {
  if (!finite(value)) return '未知'
  if (value !== 0 && Math.abs(value) < 1e-8) return `$${value.toExponential(4)}`
  return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', notation: compact && value >= 1000 ? 'compact' : 'standard', maximumFractionDigits: value > 0 && value < 0.01 ? 10 : value < 1 ? 6 : 2 }).format(value)
}
export const percentage = (value: number | null | undefined) => finite(value) ? `${value > 0 ? '+' : ''}${value.toFixed(2)}%` : '未知'
export const utc = (value: string | null | undefined) => value && Number.isFinite(Date.parse(value)) ? new Date(value).toISOString().replace('T', ' ').replace(/\.\d{3}Z$/, ' UTC') : '未知'
export const ageSeconds = (value: string | null | undefined, now: number) => value && Number.isFinite(Date.parse(value)) ? Math.max(0, (now - Date.parse(value)) / 1000) : null
export const isStale = (value: string | null | undefined, threshold: number, now: number) => { const age = ageSeconds(value, now); return age === null || age > threshold }
export const ago = (value: string | null | undefined, now: number) => { const seconds = ageSeconds(value, now); return seconds === null ? '未知时间' : seconds < 60 ? `${Math.floor(seconds)} 秒前` : seconds < 3600 ? `${Math.floor(seconds / 60)} 分钟前` : `${Math.floor(seconds / 3600)} 小时前` }
export const safeUrl = (value: string) => { try { const url = new URL(value); return url.protocol === 'https:' ? url.href : undefined } catch { return undefined } }
export const explorerUrl = (chain: string, address: string) => chain === 'solana' ? `https://solscan.io/token/${encodeURIComponent(address)}` : `https://bscscan.com/token/${encodeURIComponent(address)}`
export const tokenHash = (item: TokenView) => `#token/${encodeURIComponent(item.chain)}/${encodeURIComponent(item.token_address)}/${encodeURIComponent(item.pool_address)}`
export function parseHash(hash: string): { chain: string; address: string; pool: string } | null {
  try { const [route, chain, address, pool = ''] = hash.replace(/^#/, '').split('/').map(decodeURIComponent); return route === 'token' && ['solana', 'bsc'].includes(chain ?? '') && isAddress(chain!, address ?? '') ? { chain: chain!, address: address!, pool } : null } catch { return null }
}
export interface Filters { chain: string; text: string; label: string; minimumLiquidity: number; minimumCap: number; maximumCap: number; maximumAgeHours: number; includeUnknownCap: boolean; sort: string }
export function filterTokens(items: TokenView[], filters: Filters, now: number): TokenView[] {
  return items.filter(item => {
    const text = filters.text.trim(); const age = ageSeconds(item.pool_created_at, now)
    if (filters.chain !== 'all' && item.chain !== filters.chain) return false
    if (text && !addressEquals(item.chain, item.token_address, text) && !addressEquals(item.chain, item.pool_address, text) && !`${item.name} ${item.symbol}`.toLowerCase().includes(text.toLowerCase())) return false
    if (filters.label !== 'all' && item.label !== filters.label) return false
    if (filters.minimumLiquidity > 0 && (!finite(item.liquidity_usd) || item.liquidity_usd < filters.minimumLiquidity)) return false
    if ((filters.minimumCap > 0 || filters.maximumCap > 0) && !finite(item.market_cap_usd) && !filters.includeUnknownCap) return false
    if (finite(item.market_cap_usd) && (item.market_cap_usd < filters.minimumCap || (filters.maximumCap > 0 && item.market_cap_usd > filters.maximumCap))) return false
    if (filters.maximumAgeHours > 0 && (age === null || age > filters.maximumAgeHours * 3600)) return false
    return true
  }).sort((a, b) => {
    const field = filters.sort === 'liquidity' ? 'liquidity_usd' : filters.sort === 'volume' ? 'volume_24h_usd' : filters.sort === 'change' ? 'price_change_1h_pct' : null
    return field ? (b[field] ?? -Infinity) - (a[field] ?? -Infinity) : Date.parse(b.fetched_at) - Date.parse(a.fetched_at)
  })
}
export function chartSegments(history: Point[], field: 'price_usd' | 'liquidity_usd' = 'price_usd'): { paths: string[]; min: number; max: number; count: number; start: string; end: string } | null {
  const points = history.filter(p => Number.isFinite(Date.parse(p.at))).slice().sort((a, b) => Date.parse(a.at) - Date.parse(b.at))
  const values = points.map(p => p[field]).filter(finite)
  if (values.length < 2 || points.length < 2) return null
  const min = Math.min(...values), max = Math.max(...values), first = Date.parse(points[0]!.at), last = Date.parse(points.at(-1)!.at)
  if (last <= first) return null
  const paths: string[] = []; let segment: string[] = []
  for (const point of points) {
    if (!finite(point[field])) { if (segment.length) paths.push(segment.join(' ')); segment = []; continue }
    const x = 16 + ((Date.parse(point.at) - first) / (last - first)) * 768
    const y = max === min ? 100 : 176 - ((point[field]! - min) / (max - min)) * 152
    segment.push(`${x.toFixed(1)},${y.toFixed(1)}`)
  }
  if (segment.length) paths.push(segment.join(' '))
  return { paths, min, max, count: values.length, start: points[0]!.at, end: points.at(-1)!.at }
}

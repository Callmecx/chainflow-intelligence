import type { Snapshot, TokenView, WatchItem } from './types'

export const localMode = import.meta.env.VITE_MODE === 'LOCAL_LIVE'
async function request<T>(url: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(url, { cache: 'no-store', signal: AbortSignal.timeout(20000), ...init })
  if (!response.ok) throw new Error(`HTTP ${response.status} · ${url}`)
  return response.json() as Promise<T>
}
export async function loadSnapshot(): Promise<Snapshot> {
  const snapshot = await request<Snapshot>(localMode ? '/api/tokens' : `${import.meta.env.BASE_URL}snapshot.json`)
  if (!snapshot || !Array.isArray(snapshot.items) || !Array.isArray(snapshot.sources) || !Array.isArray(snapshot.signals) || !Array.isArray(snapshot.provider_errors) || !Number.isFinite(Date.parse(snapshot.generated_at_utc)) || !Number.isFinite(snapshot.max_staleness_seconds)) throw new Error('快照结构无效，无法验证数据时间和来源')
  return snapshot
}
export const lookup = (chain: string, address: string) => request<TokenView[]>(`/api/tokens/${encodeURIComponent(chain)}/${encodeURIComponent(address)}`)
export const loadWatchlist = () => request<WatchItem[]>('/api/watchlist')
export const setWatch = async (chain: string, address: string, enabled: boolean) => {
  const response = await fetch(enabled ? '/api/watchlist' : `/api/watchlist/${encodeURIComponent(chain)}/${encodeURIComponent(address)}`, {
    method: enabled ? 'POST' : 'DELETE', headers: { 'Content-Type': 'application/json' },
    ...(enabled ? { body: JSON.stringify({ chain, address }) } : {}), signal: AbortSignal.timeout(15000),
  })
  if (!response.ok) throw new Error(`关注更新失败：HTTP ${response.status}`)
}

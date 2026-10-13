import type { ResearchReport, ResearchWindow, Signal, SignalFactor, SignalOutcome, SignalResearch } from './types'

export function signalCategory(signal: Signal): 'MARKET_RISK' | 'DATA_FAULT' | 'OBSERVATION' | 'CONFIRMATION' {
  if (signal.category === 'DATA_FAULT' || signal.severity === 'DATA_FAULT' || signal.chain === 'system' || /^(SOURCE_|DATA_STALE|DATA_GAP)/.test(signal.rule)) return 'DATA_FAULT'
  if (signal.category) return signal.category
  return signal.severity === 'RISK' ? 'MARKET_RISK' : signal.severity === 'CONFIRMATION' ? 'CONFIRMATION' : 'OBSERVATION'
}
export const categoryLabel = (signal: Signal) => ({ MARKET_RISK: '市场风险 · RISK', DATA_FAULT: '数据故障 · DATA_FAULT', OBSERVATION: '关注 · WATCH', CONFIRMATION: '正式确认 · CONFIRMATION' })[signalCategory(signal)]
export const categoryClass = (signal: Signal) => ({ MARKET_RISK: 'risk', DATA_FAULT: 'data-fault', OBSERVATION: 'neutral', CONFIRMATION: 'accent' })[signalCategory(signal)]
export const researchWindows: ResearchWindow[] = ['1h', '2h', '6h']
export const statusLabel = (status?: string) => ({ COMPLETE: 'COMPLETE · 已完整跟踪', PENDING: 'PENDING · 等待目标窗口', MISSING: 'MISSING · 目标时点缺价', CENSORED: 'CENSORED · 观察受限', INSUFFICIENT_SAMPLE: 'INSUFFICIENT_SAMPLE · 样本不足' })[status ?? ''] ?? (status || 'UNKNOWN')
export const measured = (value: unknown) => typeof value === 'number' && Number.isFinite(value) ? value : null
export const missingRate = (value: number | null | undefined) => typeof value === 'number' && Number.isFinite(value) ? `${(value * 100).toFixed(1)}%` : 'UNKNOWN'
export function usableResearch(value: ResearchReport | null | undefined): ResearchReport | null {
  return value && Array.isArray(value.records) && Array.isArray(value.rule_stats) && Array.isArray(value.limitations) && Number.isFinite(Date.parse(value.generated_at_utc)) && Number.isFinite(value.min_sample_size) ? value : null
}

// Only publicly useful market evidence is exported. Never serialize whole snapshots,
// health/runtime/configuration objects, unknown backend fields, or private watchlists.
const evidenceFields = ['chain','token_address','pool_address','name','symbol','dex','price_usd','liquidity_usd','volume_24h_usd','market_cap_usd','fdv_usd','buys_5m','sells_5m','buys_1h','sells_1h','fetched_at','source','source_url'] as const
export function publicEvidence(evidence: Record<string, unknown> | undefined): Record<string, string | number | null> {
  return Object.fromEntries(evidenceFields.filter(key => evidence && Object.hasOwn(evidence, key)).map(key => {
    const value = evidence![key]
    return [key, typeof value === 'string' || (typeof value === 'number' && Number.isFinite(value)) ? value : null]
  }))
}
function publicFactors(factors: SignalFactor[] | undefined) {
  return (factors ?? []).map(({ name, value, weight, score, evidence }) => ({name,value,weight,score,evidence}))
}
function publicWindow(window: SignalOutcome | undefined) {
  if (!window) return null
  const { status, samples, return_pct, max_positive_pct, max_drawdown_pct, liquidity_change_pct, invalidated, target_at, observed_at, max_gap_seconds, tolerance_seconds, source_changed, reason } = window
  return {status,samples,return_pct,max_positive_pct,max_drawdown_pct,liquidity_change_pct,invalidated,target_at,observed_at,max_gap_seconds,tolerance_seconds,source_changed,reason}
}
function publicRecord(record: SignalResearch) {
  const { signal_id, chain, token_address, pool_address, rule, rule_version, trigger_at, source_at, baseline_price_usd, baseline_liquidity_usd } = record
  return {signal_id,chain,token_address,pool_address,rule,rule_version,trigger_at,source_at,baseline_price_usd,baseline_liquidity_usd,evidence_snapshot:publicEvidence(record.evidence_snapshot),factors:publicFactors(record.factors),windows:Object.fromEntries(researchWindows.map(window=>[window,publicWindow(record.windows[window])]))}
}
export function researchJson(report: ResearchReport): string {
  return JSON.stringify({kind:'SAMPLED_SIGNAL_RESEARCH',generated_at_utc:report.generated_at_utc,status:report.status,min_sample_size:report.min_sample_size,records:report.records.map(publicRecord),rule_stats:report.rule_stats.map(stat=>({rule:stat.rule,rule_version:stat.rule_version,window:stat.window,status:stat.status,trigger_count:stat.trigger_count,eligible_count:stat.eligible_count,valid_count:stat.valid_count,missing_count:stat.missing_count,censored_count:stat.censored_count,pending_count:stat.pending_count,missing_rate:stat.missing_rate,mean_return_pct:stat.mean_return_pct,median_return_pct:stat.median_return_pct,distribution:{negative:stat.distribution.negative,flat:stat.distribution.flat,positive:stat.distribution.positive}})),limitations:report.limitations},null,2)
}
function csvCell(value: unknown): string {
  if (value === null || value === undefined) return '"UNKNOWN"'
  let text = String(value)
  if (typeof value === 'string' && /^[\s]*[=+\-@]/.test(text)) text = `'${text}`
  return `"${text.replaceAll('"','""')}"`
}
export function researchCsv(report: ResearchReport): string {
  const columns = ['signal_id','chain','token_address','pool_address','rule','rule_version','trigger_at','source_at','baseline_price_usd','baseline_liquidity_usd','window','status','samples','return_pct','max_positive_pct','max_drawdown_pct','liquidity_change_pct','invalidated','target_at','observed_at','max_gap_seconds','reason']
  const rows = report.records.flatMap(record=>researchWindows.map(window=> {
    const result = record.windows[window]
    return [record.signal_id,record.chain,record.token_address,record.pool_address,record.rule,record.rule_version,record.trigger_at,record.source_at,record.baseline_price_usd,record.baseline_liquidity_usd,window,result?.status ?? 'UNKNOWN',result?.samples,result?.return_pct,result?.max_positive_pct,result?.max_drawdown_pct,result?.liquidity_change_pct,result?.invalidated,result?.target_at,result?.observed_at,result?.max_gap_seconds,result?.reason]
  }))
  return '\uFEFF' + [columns,...rows].map(row=>row.map(csvCell).join(',')).join('\r\n')
}
export function downloadResearch(report: ResearchReport, format: 'json' | 'csv') {
  const blob = new Blob([format === 'json' ? researchJson(report) : researchCsv(report)], {type:format === 'json' ? 'application/json;charset=utf-8' : 'text/csv;charset=utf-8'})
  const url = URL.createObjectURL(blob); const anchor = document.createElement('a')
  anchor.href = url; anchor.download = `chainflow-research-${report.generated_at_utc.replace(/[^0-9TZ]/g,'')}.${format}`
  anchor.click(); setTimeout(()=>URL.revokeObjectURL(url),1000)
}

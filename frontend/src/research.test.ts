import { describe, expect, it } from 'vitest'
import { fixtureResearch, fixtureSnapshot } from '../tests/fixtures'
import { publicEvidence, researchCsv, researchJson, signalCategory, statusLabel, usableResearch } from './research'
import type { ResearchReport } from './types'

describe('market risk and source fault separation',()=>{
  it('classifies V1 source failures separately from market risk',()=>{
    const signal=fixtureSnapshot().signals[0]!
    expect(signalCategory(signal)).toBe('MARKET_RISK')
    expect(signalCategory({...signal,chain:'system',rule:'SOURCE_DISCONNECTED'})).toBe('DATA_FAULT')
    expect(signalCategory({...signal,rule:'DATA_STALE'})).toBe('DATA_FAULT')
    expect(signalCategory({...signal,category:'DATA_FAULT'})).toBe('DATA_FAULT')
  })
  it('keeps observations and confirmations distinct',()=>{
    const signal=fixtureSnapshot().signals[0]!
    expect(signalCategory({...signal,severity:'WATCH'})).toBe('OBSERVATION')
    expect(signalCategory({...signal,severity:'CONFIRMATION'})).toBe('CONFIRMATION')
  })
})
describe('research data preservation and safe exports',()=>{
  it('treats empty legacy research objects as unavailable instead of enabling broken exports',()=>{
    expect(usableResearch({} as ResearchReport)).toBeNull()
    expect(usableResearch(null)).toBeNull()
    expect(usableResearch(fixtureResearch())).not.toBeNull()
  })
  it('preserves missing and future windows without manufacturing returns',()=>{
    const exported=JSON.parse(researchJson(fixtureResearch()))
    expect(exported.records[0].windows['2h'].status).toBe('MISSING')
    expect(exported.records[0].windows['2h'].return_pct).toBeNull()
    expect(exported.records[0].windows['6h'].status).toBe('PENDING')
    expect(exported.records[0].windows['6h'].return_pct).toBeNull()
    expect(exported.rule_stats[1].missing_count).toBe(1)
  })
  it('exports only explicit public research fields and strips private config everywhere',()=>{
    const report=fixtureResearch()
    Object.assign(report,{telegram:{token:'TEST_SECRET'},watchlist:['TEST_PRIVATE']})
    Object.assign(report.records[0]!,{chat_id:'TEST_SECRET'})
    Object.assign(report.records[0]!.evidence_snapshot,{TELEGRAM_BOT_TOKEN:'TEST_SECRET',runtime:{token:'TEST_SECRET'}})
    Object.assign(report.records[0]!.factors[0]!,{private:'TEST_SECRET'})
    Object.assign(report.rule_stats[0]!,{config:'TEST_SECRET'})
    expect(researchJson(report)).not.toMatch(/TEST_SECRET|TEST_PRIVATE|chat_id|TELEGRAM_BOT_TOKEN/)
    expect(researchCsv(report)).not.toMatch(/TEST_SECRET|TEST_PRIVATE/)
    expect(publicEvidence({price_usd:0,fdv_usd:null,token:'TEST_SECRET'})).toEqual({price_usd:0,fdv_usd:null})
  })
  it('retains one CSV row for every record/window including missing samples',()=>{
    const csv=researchCsv(fixtureResearch())
    expect(csv.split('\r\n')).toHaveLength(4)
    expect(csv).toContain('"MISSING"')
    expect(csv).toContain('"PENDING"')
    expect(csv).toContain('"UNKNOWN"')
  })
  it('neutralizes spreadsheet formula injection while retaining numeric negative returns',()=>{
    const report=fixtureResearch()
    report.records[0]!.rule='=HYPERLINK("https://example.com")'
    report.records[0]!.windows['1h']!.return_pct=-7
    const csv=researchCsv(report)
    expect(csv).toContain('"\'=HYPERLINK(""https://example.com"")"')
    expect(csv).toContain('"-7"')
  })
  it('keeps evidence null unknown and never serializes nested unrecognized data',()=>{
    expect(publicEvidence({price_usd:NaN,source:{token:'TEST_SECRET'},symbol:'TEST_SOL'})).toEqual({symbol:'TEST_SOL',price_usd:null,source:null})
  })
  it('keeps insufficient sample and censored states visible',()=>{
    expect(statusLabel('INSUFFICIENT_SAMPLE')).toContain('样本不足')
    expect(statusLabel('CENSORED')).toContain('观察受限')
  })
})

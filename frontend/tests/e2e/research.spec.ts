import { test, expect } from '@playwright/test'
import { readFile } from 'node:fs/promises'
import { fixtureResearch, fixtureSnapshot } from '../fixtures'

test.beforeEach(async({page})=>{
  const snapshot=fixtureSnapshot({research:fixtureResearch()})
  Object.assign(snapshot,{telegram:{token:'TEST_SECRET_MUST_NOT_RENDER'},watchlist:['TEST_PRIVATE_WATCH']})
  Object.assign(snapshot.research!,{telegram:{token:'TEST_SECRET_MUST_NOT_EXPORT'}})
  await page.route('**/snapshot.json',route=>route.fulfill({json:snapshot}))
})

test('research displays genuine missing states and compares selected rule/window without filling future data',async({page})=>{
  await page.goto('./#research')
  await expect(page.getByRole('heading',{name:'真实信号效果与规则对照'})).toBeVisible()
  await expect(page.locator('.research-meta')).toContainText('INSUFFICIENT_SAMPLE')
  await expect(page.locator('.research-table')).toContainText('TEST_v1.1')
  await page.getByRole('button',{name:'2h 结果',exact:true}).click()
  await expect(page.locator('.research-record')).toContainText('MISSING')
  await expect(page.locator('.research-table')).toContainText('100.0%')
  await page.locator('.research-record>summary').click()
  await expect(page.locator('.research-record-body')).toContainText('目标时点缺失')
  await page.getByRole('button',{name:'6h 结果',exact:true}).click()
  await expect(page.locator('.research-record')).toContainText('PENDING')
  await expect(page.locator('.research-record')).not.toContainText('+5.00%')
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth)).toBe(true)
  await page.screenshot({path:test.info().outputPath('v11-test-fixture-research.png'),fullPage:true})
})

test('public exports preserve censored records and exclude private metadata without accessing local APIs',async({page})=>{
  const apiRequests:string[]=[]
  page.on('request',request=>{if(new URL(request.url()).pathname.startsWith('/api/'))apiRequests.push(request.url())})
  await page.goto('./#research')
  const downloadPromise=page.waitForEvent('download')
  await page.getByRole('button',{name:'导出 JSON',exact:true}).click()
  const download=await downloadPromise
  const path=test.info().outputPath('test-fixture-research.json')
  await download.saveAs(path)
  const text=await readFile(path,'utf8')
  expect(text).toContain('TEST_RESEARCH_SIGNAL')
  expect(text).toContain('MISSING')
  expect(text).not.toMatch(/TEST_SECRET|TEST_PRIVATE|telegram/)
  const csvPromise=page.waitForEvent('download')
  await page.getByRole('button',{name:'导出 CSV',exact:true}).click()
  const csvDownload=await csvPromise
  const csvPath=test.info().outputPath('test-fixture-research.csv')
  await csvDownload.saveAs(csvPath)
  expect((await readFile(csvPath,'utf8')).split('\r\n')).toHaveLength(4)
  await page.getByRole('button',{name:'数据源健康'}).click()
  await expect(page.getByRole('heading',{name:'公开站的健康信息边界'})).toBeVisible()
  await expect(page.locator('body')).not.toContainText('TEST_SECRET')
  await expect(page.getByRole('heading',{name:'Telegram 安全配置状态'})).toHaveCount(0)
  expect(apiRequests).toEqual([])
})

test('signal evidence expands immutable inputs while source failure is not counted as market risk',async({page})=>{
  const snapshot=fixtureSnapshot({research:fixtureResearch()})
  const market=snapshot.signals[0]!
  Object.assign(market,{category:'MARKET_RISK',rule_version:'TEST_v1.1',source_at:market.at,baseline_price_usd:1,baseline_liquidity_usd:5000,factors:[{name:'TEST_LIQUIDITY_FACTOR',value:-31,weight:2,score:2,evidence:['TEST_INPUT_EVIDENCE']}],evidence_snapshot:{symbol:'TEST_SYMBOL',price_usd:1,secret:'TEST_SECRET_MUST_NOT_RENDER'}})
  snapshot.signals.push({...market,id:'TEST_DATA_FAULT',category:'DATA_FAULT',severity:'DATA_FAULT',rule:'SOURCE_DISCONNECTED',message:'测试数据源故障',chain:'system'})
  await page.route('**/snapshot.json',route=>route.fulfill({json:snapshot}))
  await page.goto('./')
  await expect(page.locator('.overview-stats .stat').nth(1).locator('strong')).toContainText('1')
  await expect(page.locator('.overview-stats .stat').nth(1)).toContainText('数据故障 1 条另计')
  await page.getByRole('button',{name:/预警与事件/}).click()
  const marketEvent=page.getByRole('group',{name:'信号证据 TEST_FIXTURE_SIGNAL',exact:true})
  await marketEvent.locator('summary').first().click()
  await expect(marketEvent.getByText('TEST_LIQUIDITY_FACTOR',{exact:true})).toBeVisible()
  await expect(marketEvent.getByText('TEST_INPUT_EVIDENCE',{exact:true})).toBeVisible()
  await expect(marketEvent.getByText('分数不是上涨概率',{exact:false})).toBeVisible()
  await expect(page.locator('body')).not.toContainText('TEST_SECRET')
  await expect(page.locator('.signal-event').filter({hasText:'测试数据源故障'})).toContainText('数据故障 · DATA_FAULT')
})

test('fresh source check cannot hide a stale last successful observation',async({page})=>{
  const snapshot=fixtureSnapshot()
  snapshot.sources[0]!.last_success_at='2020-01-01T00:00:00Z'
  await page.route('**/snapshot.json',route=>route.fulfill({json:snapshot}))
  await page.goto('./#sources')
  await expect(page.locator('.source-card')).toContainText('STALE · 最后检查或成功数据已过期')
})

test('legacy snapshot shows research unavailable instead of invented statistics',async({page})=>{
  await page.route('**/snapshot.json',route=>route.fulfill({json:fixtureSnapshot()}))
  await page.goto('./#research')
  await expect(page.getByText('UNAVAILABLE · 当前数据没有 V1.1 研究报告')).toBeVisible()
  await expect(page.getByRole('button',{name:'导出 JSON',exact:true})).toHaveCount(0)
})

import { test, expect } from '@playwright/test'
import { fixtureSnapshot, solAddress } from '../fixtures'

test.beforeEach(async ({ page }) => {
  await page.route('**/snapshot.json', route => route.fulfill({json:fixtureSnapshot()}))
})

test('snapshot mode identifies limited coverage, chain filtering, and unknown cap', async ({ page }) => {
  await page.goto('./')
  await expect(page.getByText('公开定时快照 · 非连续监控')).toBeVisible()
  await expect(page.getByText('TEST_FIXTURE_ONLY').first()).toBeVisible()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(2)
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true)
  await page.screenshot({path:test.info().outputPath('test-fixture-dashboard.png'),fullPage:true})
  await page.getByRole('button', {name:'Solana',exact:true}).click()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(1)
  await expect(page.locator('.market-table tbody tr').getByText('未知',{exact:true})).toBeVisible()
  await page.getByRole('button', {name:'BSC',exact:true}).click()
  await expect(page.locator('.market-table tbody tr')).toContainText('TEST_BSC')
})

test('full address search opens hash detail, evidence, sampled chart and browser watchlist', async ({ page }) => {
  await page.goto('./')
  await page.getByRole('textbox', {name:'完整代币地址',exact:true}).fill(solAddress)
  await page.getByRole('button',{name:'精确搜索 ↗'}).click()
  await expect(page).toHaveURL(/#token\/solana\//)
  await expect(page.getByRole('heading',{name:'为什么进入观察'})).toBeVisible()
  await expect(page.getByRole('heading',{name:'仍然缺少什么'})).toBeVisible()
  await expect(page.getByRole('img',{name:'真实价格采样走势'})).toBeVisible()
  await page.getByRole('button',{name:'☆ 加入关注',exact:true}).click()
  await expect(page.getByRole('status')).toContainText('公网不会持续监控')
  await page.reload()
  await expect(page.getByRole('button',{name:'★ 已关注 · 点击取消'})).toBeVisible()
  await expect(page.getByRole('link',{name:'浏览器查证 ↗'})).toHaveAttribute('href',`https://solscan.io/token/${solAddress}`)
  await page.screenshot({path:test.info().outputPath('test-fixture-detail.png'),fullPage:true})
})

test('copy retains full address and layout stays within mobile viewport', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read','clipboard-write'])
  await page.goto(`./#token/solana/${solAddress}/TEST_FIXTURE_POOL_SOL`)
  await page.getByRole('button',{name:'复制完整代币地址',exact:true}).click()
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(solAddress)
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true)
})

test('stale snapshot is visibly stale even when provider reports OK', async ({ page }) => {
  await page.route('**/snapshot.json', route => route.fulfill({json:fixtureSnapshot({generated_at_utc:'2020-01-01T00:00:00Z'})}))
  await page.goto('./')
  await expect(page.getByText('快照已陈旧 · STALE')).toBeVisible()
})

test('missing data is unavailable and never replaced by demo prices', async ({ page }) => {
  await page.route('**/snapshot.json', route => route.fulfill({status:503,body:'TEST_FIXTURE_OUTAGE'}))
  await page.goto('./')
  await expect(page.getByText('数据更新失败 · SOURCE_ERROR',{exact:true})).toBeVisible()
  await expect(page.getByText('行情暂不可用 · UNAVAILABLE')).toBeVisible()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(0)
})

test('HTTP 200 failure snapshot shows source error and unavailable rather than empty filters', async ({ page }) => {
  const snapshot = fixtureSnapshot({items:[],signals:[],provider_errors:['TEST_FIXTURE_SCAN_FAILED']})
  snapshot.sources[0] = {...snapshot.sources[0]!,status:'SOURCE_ERROR',last_success_at:null,errors:1}
  await page.route('**/snapshot.json',route=>route.fulfill({status:200,json:snapshot}))
  await page.goto('./')
  await expect(page.getByText('数据更新失败 · SOURCE_ERROR',{exact:true})).toBeVisible()
  await expect(page.getByRole('heading',{name:'行情暂不可用 · UNAVAILABLE'})).toBeVisible()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(0)
  await expect(page.getByRole('button',{name:'重置筛选',exact:true})).toHaveCount(0)
  await expect(page.getByText('当前条件下暂无交易池',{exact:true})).toHaveCount(0)
  await page.getByRole('link',{name:'查看来源异常 ↗'}).click()
  await expect(page.getByText('TEST_FIXTURE_SCAN_FAILED',{exact:true})).toBeVisible()
})

test('partial provider errors preserve received rows and warn on the homepage', async ({ page }) => {
  await page.route('**/snapshot.json',route=>route.fulfill({json:fixtureSnapshot({provider_errors:['TEST_FIXTURE_PARTIAL_FAILURE']})}))
  await page.goto('./')
  await expect(page.getByText('部分来源异常 · SOURCE_ERROR',{exact:true})).toBeVisible()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(2)
  await expect(page.locator('.mode-banner')).toContainText('仅展示已取得的行情')
  await expect(page.getByText('行情暂不可用 · UNAVAILABLE',{exact:true})).toHaveCount(0)
})

test('failed source status warns even when provider error strings are empty', async ({ page }) => {
  const snapshot = fixtureSnapshot()
  snapshot.sources.push({...snapshot.sources[0]!,id:'TEST_FIXTURE_FAILED_SOURCE',status:'UNAVAILABLE',last_success_at:null,errors:1})
  await page.route('**/snapshot.json',route=>route.fulfill({json:snapshot}))
  await page.goto('./')
  await expect(page.getByText('部分来源异常 · SOURCE_ERROR',{exact:true})).toBeVisible()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(2)
})

test('unsupported BSC log capability alone does not become a source failure', async ({ page }) => {
  const snapshot = fixtureSnapshot()
  snapshot.sources.push({...snapshot.sources[0]!,id:'bsc_event_logs',status:'UNSUPPORTED',last_success_at:null,requests:0})
  await page.route('**/snapshot.json',route=>route.fulfill({json:snapshot}))
  await page.goto('./')
  await expect(page.getByText('公开定时快照 · 非连续监控',{exact:true})).toBeVisible()
  await expect(page.locator('.mode-banner')).not.toHaveClass(/warning/)
  await expect(page.locator('.market-table tbody tr')).toHaveCount(2)
  await expect(page.getByRole('link',{name:'查看来源异常 ↗'})).toHaveCount(0)
})

test('snapshot missing requested address explains the limit', async ({ page }) => {
  await page.goto('./')
  await page.getByRole('textbox',{name:'完整代币地址',exact:true}).fill('11111111111111111111111111111111')
  await page.getByRole('button',{name:'精确搜索 ↗'}).click()
  await expect(page.getByRole('status')).toContainText('当次公开快照未收录')
  await page.getByRole('textbox',{name:'完整代币地址',exact:true}).fill('invalid')
  await page.getByRole('button',{name:'精确搜索 ↗'}).click()
  await expect(page.getByRole('status')).toContainText('地址格式不合法')
})

test('source failures and unsupported capabilities are visible', async ({ page }) => {
  await page.route('**/snapshot.json', route => route.fulfill({json:fixtureSnapshot({provider_errors:['TEST_FIXTURE_SOURCE_ERROR']})}))
  await page.goto('./')
  await page.getByRole('button',{name:'数据源健康'}).click()
  await expect(page.getByText('TEST_FIXTURE_PROVIDER',{exact:true})).toBeVisible()
  await expect(page.getByText('TEST_FIXTURE_SOURCE_ERROR',{exact:true})).toBeVisible()
  await expect(page.getByText(/BSC 官方免费端点的 eth_getLogs 为 UNSUPPORTED/)).toBeVisible()
})

test('system source outage event routes to source health instead of a fake token', async ({ page }) => {
  const snapshot = fixtureSnapshot()
  snapshot.signals = [{id:'TEST_SYSTEM_EVENT',chain:'system',token_address:'TEST_PROVIDER',pool_address:'',rule:'SOURCE_DISCONNECTED',severity:'RISK',at:new Date().toISOString(),state:'INVALIDATED',message:'测试来源失联事件',evidence:['TEST_FIXTURE_ONLY']}]
  await page.route('**/snapshot.json',route => route.fulfill({json:snapshot}))
  await page.goto('./')
  await page.getByRole('link').filter({hasText:'测试来源失联事件'}).click()
  await expect(page.getByRole('heading',{name:'实际来源探测结果'})).toBeVisible()
  await expect(page).toHaveURL(/#sources$/)
})

test('unobserved browser watchlist remains removable without fabricated market data', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('chainflow-watchlist-v1',JSON.stringify([{chain:'solana',address:'11111111111111111111111111111111'}])))
  await page.goto('./')
  await page.getByRole('button',{name:/我的关注/}).click()
  await expect(page.getByRole('heading',{name:'已关注但当前无行情'})).toBeVisible()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(0)
  await page.getByRole('button',{name:'取消关注',exact:true}).click()
  await expect(page.getByRole('heading',{name:'已关注但当前无行情'})).toHaveCount(0)
})

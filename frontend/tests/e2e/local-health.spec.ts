import { test, expect } from '@playwright/test'
import { fixtureLocalHealth, fixtureResearch, fixtureSnapshot, solAddress } from '../fixtures'

test.beforeEach(async({page})=>{
  await page.route('**/api/tokens',route=>route.fulfill({json:fixtureSnapshot({mode:'LOCAL_LIVE',research:fixtureResearch()})}))
  await page.route('**/api/watchlist',route=>route.fulfill({json:[]}))
  const health=fixtureLocalHealth()
  Object.assign(health.telegram!,{token:'TEST_SECRET_TOKEN',chat_id:'TEST_SECRET_CHAT_ID'})
  await page.route('**/api/health',route=>route.fulfill({json:health}))
})

test('local health shows unconfigured Telegram degradation and uncertain queue without exposing secrets',async({page})=>{
  await page.goto('./#sources')
  await expect(page.getByRole('heading',{name:'Telegram 安全配置状态'})).toBeVisible()
  await expect(page.locator('.local-health')).toContainText('否 / 否')
  await expect(page.locator('.local-health')).toContainText('PENDING')
  await expect(page.locator('.local-health')).toContainText('TIMEOUT_REQUIRES_REVIEW')
  await expect(page.locator('.local-health')).toContainText('ok / wal')
  await expect(page.locator('body')).not.toContainText('TEST_SECRET')
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth)).toBe(true)
  await page.screenshot({path:test.info().outputPath('v11-test-fixture-local-health.png'),fullPage:true})
})

test('health API failure does not replace real token state or invent healthy queue',async({page})=>{
  await page.route('**/api/health',route=>route.fulfill({status:503,body:'TEST_FIXTURE_HEALTH_FAILURE'}))
  await page.goto('./#sources')
  await expect(page.locator('.local-health')).toContainText('UNAVAILABLE')
  await expect(page.locator('.local-health')).toContainText('本地健康读取失败')
  await page.getByRole('button',{name:/市场发现/}).click()
  await expect(page.locator('.market-table tbody tr')).toHaveCount(2)
})

test('local watch action posts the complete chain/address and gives accurate monitoring scope',async({page})=>{
  const writes:unknown[]=[]
  await page.route('**/api/watchlist',route=>{
    if(route.request().method()==='POST'){writes.push(route.request().postDataJSON());return route.fulfill({json:{status:'WATCHING'}})}
    return route.fulfill({json:[]})
  })
  await page.goto('./')
  await page.getByRole('button',{name:'关注 TEST_SOL',exact:true}).click()
  await expect(page.getByRole('status')).toContainText('本地持续监控')
  expect(writes).toEqual([{chain:'solana',address:solAddress}])
})

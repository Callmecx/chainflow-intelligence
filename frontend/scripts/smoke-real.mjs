import { chromium, devices } from '@playwright/test'
import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'

// Explicit online smoke test. No route interception, fabricated snapshot, or fixture.
const baseURL = process.argv[2]
const outputDir = path.resolve(process.argv[3] ?? './test-results/real-smoke')
if (!baseURL || !/^https?:\/\//.test(baseURL)) throw new Error('Usage: node scripts/smoke-real.mjs <actual URL> [output directory]')
await mkdir(outputDir, { recursive: true })
const startedAt = new Date().toISOString()
const browser = await chromium.launch(process.env.PLAYWRIGHT_CHANNEL ? {channel:process.env.PLAYWRIGHT_CHANNEL} : {})
const errors = []
const results = []
let data
try {
  const desktop = await browser.newContext({ ...devices['Desktop Chrome'], permissions:['clipboard-read','clipboard-write'] })
  const page = await desktop.newPage()
  page.on('pageerror', error => errors.push(error.message))
  const response = await page.goto(baseURL)
  if (!response?.ok()) throw new Error(`Homepage HTTP ${response?.status()}`)
  await page.locator('.market-table tbody tr').first().waitFor({timeout:30000})
  const local = await page.locator('.mode-pill').innerText().then(text=>text.includes('LOCAL'))
  const dataURL = local ? new URL('/api/tokens',baseURL).href : new URL('snapshot.json',baseURL).href
  const dataResponse = await page.request.get(dataURL)
  if (!dataResponse.ok()) throw new Error(`Data endpoint HTTP ${dataResponse.status()}`)
  data = await dataResponse.json()
  if (!data.items?.length || data.items.some(item=>item.source.includes('FIXTURE') || item.discovery_labels?.some(label=>label.includes('DEMO')))) throw new Error('Expected real source data, no fixtures')
  const chains = Object.fromEntries(['solana','bsc'].map(chain=>[chain,data.items.filter(item=>item.chain===chain).length]))
  if (!chains.solana || !chains.bsc) throw new Error(`Missing dual-chain coverage: ${JSON.stringify(chains)}`)
  await page.screenshot({path:path.join(outputDir,'real-desktop-home.png')})
  await page.getByRole('button',{name:'BSC',exact:true}).click()
  const bscRows = await page.locator('.market-table tbody tr').count()
  if (!bscRows) throw new Error('BSC filter produced no rows')
  await page.getByRole('button',{name:'Solana',exact:true}).click()
  const solRows = await page.locator('.market-table tbody tr').count()
  if (!solRows) throw new Error('Solana filter produced no rows')
  const detailHash = await page.locator('.token-cell').first().getAttribute('href')
  await page.locator('.token-cell').first().click()
  await page.getByRole('heading',{name:'为什么进入观察'}).waitFor()
  const address = await page.locator('.address-panel code').first().innerText()
  await page.getByRole('button',{name:'复制完整代币地址',exact:true}).click()
  const clipboard = await page.evaluate(()=>navigator.clipboard.readText())
  if (clipboard !== address) throw new Error('Clipboard did not retain full token address')
  const explorer = await page.getByRole('link',{name:'浏览器查证 ↗'}).getAttribute('href')
  if (!explorer?.includes(encodeURIComponent(address))) throw new Error('Explorer link lost full address')
  const sourceLink = await page.locator('.facts-panel a').getAttribute('href')
  await page.getByRole('button',{name:'关闭提示'}).click()
  await page.screenshot({path:path.join(outputDir,'real-desktop-detail.png'),fullPage:true})
  await page.reload()
  await page.getByRole('heading',{name:'为什么进入观察'}).waitFor()
  results.push({surface:'desktop',homepage_http:response.status(),data_http:dataResponse.status(),mode:local?'LOCAL_LIVE':'PUBLIC_SNAPSHOT',chains,solana_filter_rows:solRows,bsc_filter_rows:bscRows,token_address:address,detail_hash:detailHash,clipboard_full_address:true,hash_refresh:true,explorer_url:explorer,source_url:sourceLink})
  const mobile = await browser.newContext({ ...devices['Pixel 7'] })
  const mobilePage = await mobile.newPage()
  mobilePage.on('pageerror', error => errors.push(error.message))
  await mobilePage.goto(baseURL)
  await mobilePage.locator('.market-table tbody tr').first().waitFor({timeout:30000})
  const mobileHomeLayout = await mobilePage.evaluate(()=>({scrollWidth:document.documentElement.scrollWidth,clientWidth:document.documentElement.clientWidth}))
  if (mobileHomeLayout.scrollWidth > mobileHomeLayout.clientWidth) throw new Error('Mobile home overflow')
  await mobilePage.screenshot({path:path.join(outputDir,'real-mobile-home.png')})
  await mobilePage.goto(new URL(detailHash,baseURL).href)
  await mobilePage.getByRole('heading',{name:'何时失效'}).waitFor()
  const mobileDetailLayout = await mobilePage.evaluate(()=>({scrollWidth:document.documentElement.scrollWidth,clientWidth:document.documentElement.clientWidth}))
  if (mobileDetailLayout.scrollWidth > mobileDetailLayout.clientWidth) throw new Error('Mobile detail overflow')
  await mobilePage.screenshot({path:path.join(outputDir,'real-mobile-detail.png'),fullPage:true})
  await mobilePage.goto(new URL('#sources',baseURL).href)
  await mobilePage.getByRole('heading',{name:'实际来源探测结果'}).waitFor()
  results.push({surface:'mobile',homepage_layout:mobileHomeLayout,detail_layout:mobileDetailLayout,source_status_page:true})
  if (errors.length) throw new Error(`Browser runtime errors: ${errors.join('; ')}`)
  const report = {kind:'REAL_ONLINE_BROWSER_SMOKE',fixture:false,started_at_utc:startedAt,finished_at_utc:new Date().toISOString(),url:baseURL,status:'PASS',generated_at_utc:data.generated_at_utc,sources:data.sources,results,browser_errors:errors,screenshots:['real-desktop-home.png','real-desktop-detail.png','real-mobile-home.png','real-mobile-detail.png']}
  await writeFile(path.join(outputDir,'real-browser-smoke.json'),JSON.stringify(report,null,2)+'\n')
  console.log(JSON.stringify(report,null,2))
} catch(error) {
  const report = {kind:'REAL_ONLINE_BROWSER_SMOKE',fixture:false,started_at_utc:startedAt,finished_at_utc:new Date().toISOString(),url:baseURL,status:'FAILED',results,browser_errors:errors,error:String(error)}
  await writeFile(path.join(outputDir,'real-browser-smoke.json'),JSON.stringify(report,null,2)+'\n')
  throw error
} finally { await browser.close() }

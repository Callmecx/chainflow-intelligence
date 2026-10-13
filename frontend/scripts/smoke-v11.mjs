import { chromium, devices } from '@playwright/test'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

// Read-only real UI acceptance. Never intercept requests or inject fixture data.
const url=process.argv[2]
const output=path.resolve(process.argv[3]??'./test-results/v11-real')
if(!url||!/^https?:\/\//.test(url))throw new Error('Usage: node scripts/smoke-v11.mjs <actual site URL> [output directory]')
await mkdir(output,{recursive:true})
const started=new Date().toISOString()
const browser=await chromium.launch(process.env.PLAYWRIGHT_CHANNEL?{channel:process.env.PLAYWRIGHT_CHANNEL}:{})
const errors=[],results=[],screenshots=[]
const capture=async(page,name)=>{await page.screenshot({path:path.join(output,name)});screenshots.push(name)}
try{
  const context=await browser.newContext({...devices['Desktop Chrome'],acceptDownloads:true})
  const page=await context.newPage()
  page.on('pageerror',error=>errors.push(error.message))
  const response=await page.goto(url)
  if(!response?.ok())throw new Error(`Homepage HTTP ${response?.status()}`)
  await page.locator('.market-table tbody tr').first().waitFor({timeout:30000})
  const local=(await page.locator('.mode-pill').innerText()).includes('LOCAL')
  const payloadURL=local?new URL('/api/tokens',url).href:new URL('snapshot.json',url).href
  const payloadResponse=await page.request.get(payloadURL)
  const payload=await payloadResponse.json()
  if(payload.mode!==(local?'LOCAL_LIVE':'PUBLIC_SNAPSHOT'))throw new Error('Unexpected snapshot mode')
  if(!local && ['telegram','watchlist','runtime','notification_counts'].some(key=>Object.hasOwn(payload,key)||Object.hasOwn(payload.metrics??{},key)))throw new Error('Public snapshot exposes local/private status fields')
  if(!payload.research||!Array.isArray(payload.research.records)||!Array.isArray(payload.research.rule_stats))throw new Error('V1.1 research report unavailable')
  await capture(page,'v11-real-desktop-home.png')
  await page.getByRole('button',{name:'信号研究',exact:true}).click()
  await page.getByRole('heading',{name:'真实信号效果与规则对照'}).waitFor()
  await capture(page,'v11-real-desktop-research.png')
  const downloadPromise=page.waitForEvent('download')
  await page.getByRole('button',{name:'导出 JSON',exact:true}).click()
  const download=await downloadPromise
  const exportPath=path.join(output,'v11-real-public-research-export.json')
  await download.saveAs(exportPath)
  const research=JSON.parse(await readFile(exportPath,'utf8'))
  if(research.records.length!==payload.research.records.length)throw new Error('Export lost research rows')
  if(/"(?:telegram|bot_token|chat_id|watchlist|runtime)"\s*:/i.test(JSON.stringify(research)))throw new Error('Export includes forbidden private fields')
  const windows=Object.fromEntries(['1h','2h','6h'].map(window=>[window,research.records.reduce((counts,row)=>{const state=row.windows[window]?.status??'UNKNOWN';counts[state]=(counts[state]??0)+1;return counts},{})]))
  results.push({surface:'desktop',mode:payload.mode,homepage_http:response.status(),payload_http:payloadResponse.status(),generated_at_utc:payload.generated_at_utc,items:payload.items.length,research_status:research.status,records:research.records.length,window_status_counts:windows,json_export_same_record_count:true,private_export_fields:false})
  await page.getByRole('button',{name:/预警与事件/}).click()
  const event=payload.signals.find(signal=>signal.rule_version)||payload.signals[0]
  if(event){
    const evidence=page.getByRole('group',{name:`信号证据 ${event.id}`,exact:true})
    await evidence.locator('summary').first().click()
    await evidence.screenshot({path:path.join(output,'v11-real-signal-evidence.png')})
    screenshots.push('v11-real-signal-evidence.png')
    results.push({surface:'signal_evidence',signal_id:event.id,rule:event.rule,rule_version:event.rule_version||'V1_LEGACY_UNKNOWN',category:event.category||'V1_LEGACY',at:event.at,expanded:true})
  }
  await page.getByRole('button',{name:'数据源健康'}).click()
  if(local){
    await page.getByRole('heading',{name:'Telegram 安全配置状态'}).waitFor()
    const healthResponse=await page.request.get(new URL('/api/health',url).href)
    const health=await healthResponse.json()
    if(!health.telegram)throw new Error('V1.1 local notification status unavailable')
    if('token' in health.telegram || 'chat_id' in health.telegram || 'bot_token' in health.telegram)throw new Error('Health contains forbidden Telegram fields')
    results.push({surface:'local_health',status:health.status,database:health.database?.integrity_check,telegram_configured:health.telegram.configured,telegram_enabled:health.telegram.enabled,telegram_verification:health.telegram.verification,credential_fields_present:false})
  }else await page.getByRole('heading',{name:'公开站的健康信息边界'}).waitFor()
  await capture(page,'v11-real-desktop-health.png')
  const mobile=await browser.newContext({...devices['Pixel 7']})
  const mobilePage=await mobile.newPage()
  mobilePage.on('pageerror',error=>errors.push(error.message))
  for(const section of ['research','sources']){
    await mobilePage.goto(new URL(`#${section}`,url).href)
    await mobilePage.getByRole('heading',{name:section==='research'?'真实信号效果与规则对照':'实际来源探测结果'}).waitFor()
    const layout=await mobilePage.evaluate(()=>({clientWidth:document.documentElement.clientWidth,scrollWidth:document.documentElement.scrollWidth}))
    if(layout.scrollWidth>layout.clientWidth)throw new Error(`Mobile ${section} overflow`)
    await capture(mobilePage,`v11-real-mobile-${section}.png`)
    results.push({surface:`mobile_${section}`,layout})
  }
  if(errors.length)throw new Error(`Runtime errors: ${errors.join('; ')}`)
  const report={kind:'V1_1_REAL_BROWSER_SMOKE',fixture:false,started_at_utc:started,finished_at_utc:new Date().toISOString(),url,status:'PASS',results,browser_errors:errors,screenshots}
  await writeFile(path.join(output,'v11-real-browser-smoke.json'),JSON.stringify(report,null,2)+'\n')
  console.log(JSON.stringify(report,null,2))
}catch(error){
  await writeFile(path.join(output,'v11-real-browser-smoke.json'),JSON.stringify({kind:'V1_1_REAL_BROWSER_SMOKE',fixture:false,started_at_utc:started,finished_at_utc:new Date().toISOString(),url,status:'FAILED',results,browser_errors:errors,error:String(error)},null,2)+'\n')
  throw error
}finally{await browser.close()}

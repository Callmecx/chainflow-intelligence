<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue'
import { loadLocalHealth, loadSnapshot, loadWatchlist, localMode, lookup, setWatch } from './api'
import { addressEquals, ago, explorerUrl, filterTokens, isAddress, isStale, keyOf, money, parseHash, percentage, safeUrl, shortAddress, tokenHash, utc, type Filters } from './domain'
import type { LocalHealth, Snapshot, SourceHealth, TokenView, WatchItem } from './types'
import SampledChart from './components/SampledChart.vue'
import SignalEvent from './components/SignalEvent.vue'
import ResearchPanel from './components/ResearchPanel.vue'
import LocalHealthPanel from './components/LocalHealthPanel.vue'
import { categoryLabel, signalCategory, usableResearch } from './research'

const snapshot = ref<Snapshot | null>(null)
const extraItems = ref<TokenView[]>([])
const busy = ref(false)
const searching = ref(false)
const error = ref('')
const health = ref<LocalHealth | null>(null)
const healthError = ref('')
const notice = ref('')
const now = ref(Date.now())
const view = ref<'discover' | 'watchlist' | 'signals' | 'sources' | 'research'>('discover')
const route = ref(parseHash(window.location.hash))
const watchlist = ref<WatchItem[]>([])
const watchBusy = ref(false)
const exactAddress = ref('')
const searchChain = ref('solana')
const filters = reactive<Filters>({ chain: 'all', text: '', label: 'all', minimumLiquidity: 0, minimumCap: 0, maximumCap: 0, maximumAgeHours: 0, includeUnknownCap: true, sort: 'recent' })
const filtersOpen = ref(false)
let refreshTimer: ReturnType<typeof setInterval> | undefined
let clockTimer: ReturnType<typeof setInterval> | undefined
let noticeTimer: ReturnType<typeof setTimeout> | undefined

const items = computed(() => [...new Map([...extraItems.value, ...(snapshot.value?.items ?? [])].map(item => [keyOf(item), item])).values()])
const hasWatch = (item: TokenView) => watchlist.value.some(w => w.chain === item.chain && addressEquals(item.chain, w.address, item.token_address))
const unobservedWatchlist = computed(() => watchlist.value.filter(w => !items.value.some(item => item.chain === w.chain && addressEquals(w.chain,item.token_address,w.address))))
const visibleItems = computed(() => filterTokens(view.value === 'watchlist' ? items.value.filter(hasWatch) : items.value, filters, now.value))
const selected = computed(() => route.value ? items.value.find(item => item.chain === route.value!.chain && addressEquals(item.chain, item.token_address, route.value!.address) && (!route.value!.pool || item.pool_address === route.value!.pool)) ?? null : null)
const siblingPools = computed(() => selected.value ? items.value.filter(item => item.chain === selected.value!.chain && addressEquals(item.chain, item.token_address, selected.value!.token_address)) : [])
const stale = computed(() => snapshot.value ? isStale(snapshot.value.generated_at_utc, snapshot.value.max_staleness_seconds, now.value) : false)
const signals = computed(() => (snapshot.value?.signals ?? []).slice().sort((a,b) => Date.parse(b.at) - Date.parse(a.at)))
const riskSignals = computed(() => signals.value.filter(s => signalCategory(s) === 'MARKET_RISK'))
const dataFaultSignals = computed(() => signals.value.filter(s => signalCategory(s) === 'DATA_FAULT'))
const research = computed(()=>usableResearch(snapshot.value?.research))
const sourceHealth = (id: string) => ((snapshot.value?.metrics?.source_health ?? []) as SourceHealth[]).find(row=>row.id === id)
const validSources = computed(() => snapshot.value?.sources.filter(s => s.status === 'OK').length ?? 0)
const tokenStale = (item: TokenView) => isStale(item.fetched_at, snapshot.value?.max_staleness_seconds ?? 300, now.value)
const currentTitle = computed(() => ({ discover: '发现观察机会', watchlist: '我的关注', signals: '预警与事件', sources: '数据源健康', research: '信号研究与对照' })[view.value])
const outcomeWindows = computed(() => (selected.value?.outcome?.windows ?? {}) as Record<string, { status: string; samples: number; max_positive_pct: number | null; max_negative_pct: number | null; coverage_seconds: number; reason?: string }>)

function toast(message: string) { notice.value = message; if (noticeTimer) clearTimeout(noticeTimer); noticeTimer = setTimeout(() => { notice.value = '' }, 6000) }
async function refresh() {
  if (busy.value) return
  busy.value = true
  const [snapshotResult, healthResult] = await Promise.allSettled([loadSnapshot(),localMode ? loadLocalHealth() : Promise.resolve(null)] as const)
  if (snapshotResult.status === 'fulfilled') { snapshot.value = snapshotResult.value; error.value = '' }
  else error.value = `数据更新失败 / SOURCE_ERROR：${snapshotResult.reason instanceof Error ? snapshotResult.reason.message : String(snapshotResult.reason)}`
  if (healthResult.status === 'fulfilled') { health.value = healthResult.value; healthError.value = '' }
  else healthError.value = `本地健康读取失败：${healthResult.reason instanceof Error ? healthResult.reason.message : String(healthResult.reason)}`
  busy.value = false
}
function loadBrowserWatchlist() {
  try { const value = JSON.parse(localStorage.getItem('chainflow-watchlist-v1') ?? '[]'); watchlist.value = Array.isArray(value) ? value.filter(w => w && ['bsc', 'solana'].includes(w.chain) && typeof w.address === 'string' && isAddress(w.chain, w.address)) : [] }
  catch { toast('无法读取浏览器收藏；当前会话仍可使用。') }
}
async function toggleWatch(item: Pick<TokenView, 'chain' | 'token_address'>) {
  if (watchBusy.value) return
  watchBusy.value = true
  const enabled = !watchlist.value.some(w => w.chain === item.chain && addressEquals(item.chain, w.address, item.token_address))
  try {
    if (localMode) await setWatch(item.chain, item.token_address, enabled)
    const updated = watchlist.value.filter(w => !(w.chain === item.chain && addressEquals(item.chain, w.address, item.token_address)))
    if (enabled) updated.push({ chain: item.chain, address: item.token_address })
    watchlist.value = updated
    if (!localMode) { try { localStorage.setItem('chainflow-watchlist-v1', JSON.stringify(updated)) } catch { toast('浏览器禁止存储；收藏仅在当前会话有效。'); return } }
    toast(localMode ? (enabled ? '已加入本地持续监控；服务运行时生效。' : '已移出优先关注，发现引擎仍可能采样此币。') : (enabled ? '已收藏到此浏览器；公网不会持续监控该代币。' : '已取消浏览器收藏。'))
  } catch (err) { toast(err instanceof Error ? err.message : '关注更新失败') }
  finally { watchBusy.value = false }
}
async function copy(value: string) {
  try { await navigator.clipboard.writeText(value); toast(`已复制完整地址：${value}`) }
  catch { toast(`复制失败，请手动选择完整地址：${value}`) }
}
async function searchExact() {
  const address = exactAddress.value.trim()
  if (!isAddress(searchChain.value, address)) { toast('地址格式不合法：请选择对应链并输入完整 Mint / 合约地址。'); return }
  searching.value = true
  try {
    let found = items.value.filter(item => item.chain === searchChain.value && addressEquals(item.chain, item.token_address, address))
    if (localMode) { found = await lookup(searchChain.value, address); extraItems.value = [...extraItems.value.filter(item => !(item.chain === searchChain.value && addressEquals(item.chain, item.token_address, address))), ...found] }
    if (found.length) window.location.hash = tokenHash(found[0]!)
    else toast(localMode ? '真实来源未返回该代币的可用池，请检查来源状态和地址。' : '当次公开快照未收录该地址。请启动本地服务进行实时查询。')
  } catch (err) { toast(`查询失败：${err instanceof Error ? err.message : String(err)}`) }
  finally { searching.value = false }
}
async function syncHash() {
  route.value = parseHash(window.location.hash)
  if (window.location.hash === '#sources') view.value = 'sources'
  if (window.location.hash === '#research') view.value = 'research'
  if (route.value && localMode && !selected.value) {
    const requested = { ...route.value }; searching.value = true
    try { const result = await lookup(requested.chain, requested.address); extraItems.value = [...extraItems.value, ...result] }
    catch (err) { toast(`详情读取失败：${err instanceof Error ? err.message : String(err)}`) }
    finally { searching.value = false }
  }
}
function navigate(next: typeof view.value) { view.value = next; window.location.hash = ''; route.value = null }
function selectPool(event: Event) { const pool = siblingPools.value.find(p => keyOf(p) === (event.target as HTMLSelectElement).value); if (pool) window.location.hash = tokenHash(pool) }
function setPreset(name: string) {
  filters.minimumLiquidity = name === 'all' ? 0 : 3000
  filters.minimumCap = name === 'core' ? 5000 : name === 'growth' ? 100000 : name === 'large' ? 1000000 : 0
  filters.maximumCap = name === 'core' ? 200000 : name === 'growth' ? 1000000 : name === 'large' ? 2000000 : 0
  filters.maximumAgeHours = name === 'core' ? 4 : 0; filters.includeUnknownCap = name === 'all'; filtersOpen.value = true
}
const signalHash = (signal: {chain:string;token_address:string;pool_address:string}) => isAddress(signal.chain, signal.token_address) ? `#token/${encodeURIComponent(signal.chain)}/${encodeURIComponent(signal.token_address)}/${encodeURIComponent(signal.pool_address)}` : '#sources'
onMounted(async () => {
  if (localMode) { try { watchlist.value = await loadWatchlist() } catch { toast('本地关注列表读取失败，请确认 Rust 服务运行。') } } else loadBrowserWatchlist()
  await refresh(); await syncHash()
  refreshTimer = setInterval(refresh, localMode ? 30000 : 120000)
  clockTimer = setInterval(() => { now.value = Date.now() }, 1000)
  window.addEventListener('hashchange', syncHash)
})
onUnmounted(() => { clearInterval(refreshTimer); clearInterval(clockTimer); clearTimeout(noticeTimer); window.removeEventListener('hashchange', syncHash) })
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <a class="brand" href="#" @click.prevent="navigate('discover')"><span class="brand-mark">ϟ</span><span>ChainFlow<small>INTELLIGENCE</small></span></a>
      <div class="workspace-label">研究工作台 <span>01</span></div>
      <nav aria-label="主导航">
        <button :class="{active:view === 'discover' && !route}" @click="navigate('discover')"><span class="nav-icon">◫</span>市场发现 <span class="nav-arrow">↗</span></button>
        <button :class="{active:view === 'watchlist' && !route}" @click="navigate('watchlist')"><span class="nav-icon">☆</span>我的关注 <span class="nav-count">{{ watchlist.length }}</span></button>
        <button :class="{active:view === 'signals' && !route}" @click="navigate('signals')"><span class="nav-icon">⌁</span>预警与事件 <span v-if="riskSignals.length" class="nav-count risk-text">{{ riskSignals.length }}</span></button>
        <button :class="{active:view === 'research' && !route}" @click="navigate('research')"><span class="nav-icon">▥</span>信号研究</button>
        <button :class="{active:view === 'sources' && !route}" @click="navigate('sources')"><span class="nav-icon">⊙</span>数据源健康</button>
      </nav>
      <div class="sidebar-note"><span class="status-dot"/> 公开数据 · 只读研究<p>SOL / BSC<br>规则可解释，证据可查证。</p></div>
      <div class="sidebar-bottom"><a href="https://github.com/Callmecx/chainflow-intelligence" target="_blank" rel="noopener noreferrer">源代码与中文文档 <span>↗</span></a><p>开源研究看板 · MIT<br>零 API 订阅费用</p></div>
    </aside>
    <main>
      <header class="topbar"><div class="breadcrumb">工作台 <span>/</span> {{ route ? '代币研究' : currentTitle }}</div><div class="topbar-right"><span class="mode-pill" :class="{local:localMode}"><i/>{{ localMode ? 'LOCAL · 本地监控' : 'PUBLIC · 定时快照' }}</span><button class="icon-button refresh-button" :disabled="busy" aria-label="刷新数据" @click="refresh">↻ <span>{{ busy ? '读取中' : '刷新' }}</span></button></div></header>
      <div class="main-content">
        <div class="mode-banner" :class="{warning:stale || !!error}"><span class="banner-icon">{{ stale || error ? '!' : '◷' }}</span><div><strong>{{ error ? '数据更新失败 · SOURCE_ERROR' : stale ? '快照已陈旧 · STALE' : localMode ? '本地 Rust 持续采集模式' : '公开定时快照 · 非连续监控' }}</strong><p>{{ localMode ? '界面每 30 秒读取本地采集结果；持续监控依赖本机 Rust 进程运行。' : 'GitHub Actions 目标每半小时更新，调度可能延迟。浏览器收藏不触发后台监控；在线搜索仅覆盖当次快照。' }}</p><p v-if="snapshot" class="mono">生成时间 {{ utc(snapshot.generated_at_utc) }} · {{ ago(snapshot.generated_at_utc,now) }} · 陈旧阈值 {{ snapshot.max_staleness_seconds }} 秒</p><p v-if="error" class="error-detail">{{ error }}{{ snapshot ? ' · 保留上次结果，请核对数据时间。' : ' · 暂无可验证的行情数据。' }}</p></div><span v-if="snapshot && !error && !stale" class="badge neutral">{{ snapshot.mode }}</span></div>

        <template v-if="route">
          <button class="back-button" @click="navigate('discover')">← 返回市场发现</button>
          <template v-if="selected">
            <div class="detail-heading"><div class="token-identity"><span class="token-avatar large" :class="selected.chain">{{ selected.symbol.slice(0,2) || '?' }}</span><div><div class="eyebrow">{{ selected.chain === 'solana' ? 'SOLANA' : 'BNB SMART CHAIN' }} / {{ selected.dex }}</div><h1>{{ selected.symbol || '未知符号' }} <span>{{ selected.name }}</span></h1></div></div><button class="primary-button" :disabled="watchBusy" @click="toggleWatch(selected)">{{ hasWatch(selected) ? '★ 已关注 · 点击取消' : '☆ 加入关注' }}</button></div>
            <div class="detail-badges"><span class="badge" :class="selected.label === 'RISK' ? 'risk' : 'accent'">{{ selected.label }}</span><span class="badge neutral">{{ selected.state }}</span><span v-for="label in selected.discovery_labels" :key="label" class="badge" :class="label.includes('BOOST') ? 'boost' : 'neutral'">{{ label }}</span><span v-if="tokenStale(selected)" class="badge risk">STALE</span></div>
            <div class="address-panel panel"><div><span class="eyebrow">TOKEN / 完整地址</span><code>{{ selected.token_address }}</code><button class="text-button" aria-label="复制完整代币地址" @click="copy(selected.token_address)">复制 ↗</button><a :href="explorerUrl(selected.chain, selected.token_address)" target="_blank" rel="noopener noreferrer">浏览器查证 ↗</a></div><div><span class="eyebrow">POOL / 完整池地址</span><code>{{ selected.pool_address }}</code><button class="text-button" aria-label="复制完整池地址" @click="copy(selected.pool_address)">复制 ↗</button></div><div v-if="siblingPools.length > 1"><label for="pool-select">同币种交易池</label><select id="pool-select" :value="keyOf(selected)" @change="selectPool"><option v-for="pool in siblingPools" :key="keyOf(pool)" :value="keyOf(pool)">{{ pool.dex }} · {{ shortAddress(pool.pool_address) }}</option></select></div></div>
            <div class="detail-metrics"><div class="stat"><span>当前价格 / USD</span><strong>{{ money(selected.price_usd,false) }}</strong><small :class="(selected.price_change_1h_pct ?? 0) < 0 ? 'risk-text' : 'positive'">1h {{ percentage(selected.price_change_1h_pct) }}</small></div><div class="stat"><span>池流动性</span><strong>{{ money(selected.liquidity_usd) }}</strong><small>该交易池的公开估值</small></div><div class="stat"><span>24h 成交量</span><strong>{{ money(selected.volume_24h_usd) }}</strong><small>来源报告的滚动统计</small></div><div class="stat"><span>市值 / Market Cap</span><strong>{{ money(selected.market_cap_usd) }}</strong><small>FDV {{ money(selected.fdv_usd) }}</small></div></div>
            <div class="detail-grid"><SampledChart :history="selected.history"/><section class="panel facts-panel"><div class="panel-heading"><h3>交易与可验证性</h3></div><dl><div><dt>5m 买 / 卖笔数</dt><dd><b class="positive">{{ selected.buys_5m ?? '未知' }}</b> / <b class="risk-text">{{ selected.sells_5m ?? '未知' }}</b></dd></div><div><dt>1h 买 / 卖笔数</dt><dd>{{ selected.buys_1h ?? '未知' }} / {{ selected.sells_1h ?? '未知' }}</dd></div><div><dt>池创建时间</dt><dd>{{ utc(selected.pool_created_at) }}</dd></div><div><dt>首次观察</dt><dd>{{ utc(selected.first_seen_at) }}</dd></div><div><dt>数据获取</dt><dd>{{ utc(selected.fetched_at) }}</dd></div><div><dt>来源</dt><dd><a v-if="safeUrl(selected.source_url)" :href="safeUrl(selected.source_url)" target="_blank" rel="noopener noreferrer">{{ selected.source }} ↗</a><span v-else>{{ selected.source }}</span></dd></div><div><dt>蜜罐 / Top10 / 聪明钱</dt><dd class="muted">UNKNOWN</dd></div></dl><p class="fine-print">买卖笔数并非净资金流；UNKNOWN 代表未独立验证，不能解读为安全。</p></section></div>
            <div class="explanation-grid"><section class="panel explanation"><span class="eyebrow positive">01 / WHY</span><h3>为什么进入观察</h3><ul v-if="selected.why.length"><li v-for="reason in selected.why" :key="reason">{{ reason }}</li></ul><p v-else class="muted">暂无可验证的触发原因。</p></section><section class="panel explanation"><span class="eyebrow">02 / EVIDENCE</span><h3>已知证据</h3><ul v-if="selected.evidence.length"><li v-for="evidence in selected.evidence" :key="evidence">{{ evidence }}</li></ul><p v-else class="muted">暂无证据记录。</p></section><section class="panel explanation"><span class="eyebrow amber">03 / INCOMPLETE_DATA</span><h3>仍然缺少什么</h3><ul v-if="selected.incomplete_data.length"><li v-for="missing in selected.incomplete_data" :key="missing">{{ missing }}</li></ul><p v-else class="muted">规则未列出其他缺口；链上风险指标仍为 UNKNOWN。</p></section><section class="panel explanation"><span class="eyebrow risk-text">04 / INVALIDATION</span><h3>何时失效</h3><ul v-if="selected.invalidation.length"><li v-for="reason in selected.invalidation" :key="reason">{{ reason }}</li></ul><p v-else class="muted">尚无失效条件记录。</p></section></div>
            <section class="panel"><div class="panel-heading"><div><span class="eyebrow">FORWARD OBSERVATION</span><h3>触发后真实采样结果</h3></div><span class="badge neutral">基线：首次观察价格</span></div><div v-if="Object.keys(outcomeWindows).length" class="outcomes"><div v-for="(outcome,window) in outcomeWindows" :key="window"><span class="eyebrow">{{ window }} 观察窗口</span><strong :class="outcome.status === 'COMPLETE' ? 'positive' : 'muted'">{{ outcome.status }}</strong><p>最大正向 {{ percentage(outcome.max_positive_pct) }} / 最大负向 {{ percentage(outcome.max_negative_pct) }}</p><small>{{ outcome.samples }} 个采样 · 覆盖 {{ outcome.coverage_seconds }} 秒</small><p v-if="outcome.reason" class="fine-print">{{ outcome.reason }}</p></div></div><div v-else class="empty"><p>UNKNOWN · 尚无完整的后续采样窗口。</p></div><p class="panel-note">这是离散采样观察，不是可成交收益；缺口内的极值未知，未来时间窗口不会补造数据。</p></section>
            <section class="panel"><div class="panel-heading"><div><span class="eyebrow">SIGNAL HISTORY</span><h3>信号时间线</h3></div><button class="text-button" @click="navigate('research')">查看正式信号研究 ↗</button></div><div v-if="selected.timeline.length" class="signal-events"><SignalEvent v-for="signal in selected.timeline" :key="signal.id" :signal="signal"/></div><div v-else class="empty"><p>暂无历史状态事件。</p></div></section>
          </template>
          <section v-else class="panel empty"><span class="empty-icon">⌕</span><h2>{{ searching ? '正在查询真实来源…' : '当前数据中没有该交易池' }}</h2><p>{{ localMode ? '请检查来源健康状态或重新输入完整代币地址。' : '公开快照覆盖有限，该地址可能未被当次扫描收录。请使用本地服务精确查询。' }}</p></section>
        </template>

        <template v-else>
          <div class="page-heading"><div><div class="eyebrow">ON-CHAIN RESEARCH TERMINAL</div><h1>{{ currentTitle }}<span class="heading-period">.</span></h1><p>跟踪变化，查证证据，让风险先于信号。</p></div><span class="date-label">{{ new Date(now).toLocaleDateString('zh-CN',{year:'numeric',month:'long',day:'numeric',timeZone:'Asia/Shanghai'}) }} <small>UTC+8</small></span></div>
          <div class="overview-stats"><div class="stat"><span>已收录交易池 <span>↗</span></span><strong>{{ snapshot ? items.length : '—' }}<small>POOLS</small></strong><small>SOL {{ snapshot ? items.filter(t => t.chain === 'solana').length : '—' }} / BSC {{ snapshot ? items.filter(t => t.chain === 'bsc').length : '—' }}</small></div><div class="stat"><span>高风险事件 <span class="risk-text">◈</span></span><strong :class="{ 'risk-text':riskSignals.length }">{{ snapshot ? riskSignals.length : '—' }}<small>RISK</small></strong><small>市场风险 · 数据故障 {{ snapshot ? dataFaultSignals.length : '—' }} 条另计</small></div><div class="stat"><span>可用数据源 <span class="positive">⊙</span></span><strong>{{ snapshot ? validSources : '—' }}<small>/ {{ snapshot ? snapshot.sources.length : '—' }}</small></strong><small>基于最近一次采集探测结果</small></div><div class="stat"><span>{{ localMode ? '本地监控列表' : '浏览器收藏' }} <span>☆</span></span><strong>{{ watchlist.length }}<small>TOKENS</small></strong><small>{{ localMode ? '服务运行期间持续观察' : '收藏仅保存在当前浏览器' }}</small></div></div>

          <template v-if="view === 'discover' || view === 'watchlist'">
            <form class="search-panel" @submit.prevent="searchExact"><span class="search-symbol">⌕</span><select v-model="searchChain" aria-label="精确搜索链"><option value="solana">SOL</option><option value="bsc">BSC</option></select><input v-model="exactAddress" aria-label="完整代币地址" placeholder="输入完整 Mint / 合约地址，精确查证代币…" autocomplete="off" spellcheck="false"/><button class="primary-button" :disabled="searching || !exactAddress.trim()" type="submit">{{ searching ? '查询中…' : '精确搜索 ↗' }}</button></form>
            <p v-if="view === 'watchlist'" class="watch-note">{{ localMode ? '监控清单保存在本地 SQLite；停止 Rust 进程后监控停止。' : '这里的收藏只存在于此浏览器，不会被 GitHub Pages 后台持续监控。快照中未收录的已收藏代币不会显示行情。' }}</p>
            <section v-if="view === 'watchlist' && unobservedWatchlist.length" class="panel unavailable-watchlist"><div class="panel-heading"><h3>已关注但当前无行情</h3></div><article v-for="watch in unobservedWatchlist" :key="`${watch.chain}/${watch.address}`"><span class="badge neutral">{{ watch.chain.toUpperCase() }} · UNAVAILABLE</span><code>{{ watch.address }}</code><button class="text-button" @click="copy(watch.address)">复制完整地址</button><button class="text-button" :disabled="watchBusy" @click="toggleWatch({chain:watch.chain,token_address:watch.address})">取消关注</button></article></section>
            <section class="panel market-panel"><div class="market-toolbar"><div class="segmented"><button :class="{active:filters.chain === 'all'}" @click="filters.chain = 'all'">全部链</button><button :class="{active:filters.chain === 'solana'}" @click="filters.chain = 'solana'"><span class="chain-dot solana"/> Solana</button><button :class="{active:filters.chain === 'bsc'}" @click="filters.chain = 'bsc'"><span class="chain-dot bsc"/> BSC</button></div><div class="toolbar-controls"><input v-model="filters.text" class="filter-search" aria-label="筛选名称或完整地址" placeholder="筛选名称 / 地址"/><button class="secondary-button" :aria-expanded="filtersOpen" @click="filtersOpen = !filtersOpen">☷ 筛选条件</button><select v-model="filters.sort" aria-label="排序方式"><option value="recent">最近更新</option><option value="liquidity">流动性最高</option><option value="volume">成交量最高</option><option value="change">1h 涨幅最高</option></select></div></div>
              <div class="preset-row"><span>观察视图</span><button @click="setPreset('all')">全部收录</button><button @click="setPreset('core')">新池 · $5K–200K</button><button @click="setPreset('growth')">$100K–1M</button><button @click="setPreset('large')">$1M–2M</button><span class="result-count">{{ visibleItems.length }} 个交易池</span></div>
              <div v-if="filtersOpen" class="filter-drawer"><label>最低流动性 / USD<input v-model.number="filters.minimumLiquidity" type="number" min="0" step="1000"/></label><label>最低市值 / USD<input v-model.number="filters.minimumCap" type="number" min="0" step="1000"/></label><label>最高市值 / USD（0 不限）<input v-model.number="filters.maximumCap" type="number" min="0" step="1000"/></label><label>最大池年龄 / 小时（0 不限）<input v-model.number="filters.maximumAgeHours" type="number" min="0" step="1"/></label><label>观察标签<select v-model="filters.label"><option value="all">全部标签</option><option v-for="label in ['NEW','ACTIVE','REVIVAL_WATCH','RANGE_WATCH','MOMENTUM_WATCH','RISK','EXPIRED']" :key="label">{{ label }}</option></select></label><label class="checkbox-label"><input v-model="filters.includeUnknownCap" type="checkbox"/> 保留市值未知的池</label><p>这些条件仅筛选当前界面，不改变后端预警规则。规则阈值请在本地配置中调整；观察标签不是投资建议。</p></div>
              <div v-if="visibleItems.length" class="table-scroll"><table class="market-table"><thead><tr><th>代币 / 交易池</th><th>价格 / 1h</th><th>流动性</th><th>24h 量</th><th>市值 / FDV</th><th>观察状态</th><th>数据时间</th><th><span class="sr-only">关注</span></th></tr></thead><tbody><tr v-for="item in visibleItems" :key="keyOf(item)"><td><a :href="tokenHash(item)" class="token-cell"><span class="token-avatar" :class="item.chain">{{ item.symbol.slice(0,2) || '?' }}</span><span><strong>{{ item.symbol || '未知符号' }}<span class="chain-label">{{ item.chain === 'solana' ? 'SOL' : 'BSC' }}</span></strong><small>{{ item.name }} · {{ item.dex }}</small><code>{{ shortAddress(item.token_address) }}</code><small>池 {{ shortAddress(item.pool_address) }}</small></span></a></td><td class="numeric"><strong>{{ money(item.price_usd,false) }}</strong><small :class="(item.price_change_1h_pct ?? 0) < 0 ? 'risk-text' : item.price_change_1h_pct === null ? 'muted' : 'positive'">{{ percentage(item.price_change_1h_pct) }}</small></td><td class="numeric">{{ money(item.liquidity_usd) }}</td><td class="numeric">{{ money(item.volume_24h_usd) }}</td><td class="numeric"><strong>{{ money(item.market_cap_usd) }}</strong><small>FDV {{ money(item.fdv_usd) }}</small></td><td><span class="badge" :class="item.label === 'RISK' ? 'risk' : item.label === 'EXPIRED' ? 'neutral' : 'accent'">{{ item.label }}</span><small class="state-label">{{ item.state }}</small><span v-if="item.discovery_labels.some(l => l.includes('BOOST'))" class="badge boost">BOOSTED</span></td><td><span :class="tokenStale(item) ? 'risk-text' : 'muted'">{{ tokenStale(item) ? 'STALE · ' : '' }}{{ ago(item.fetched_at,now) }}</span><small class="source-label">{{ item.source }}</small></td><td><button class="star-button" :class="{selected:hasWatch(item)}" :aria-label="`${hasWatch(item) ? '取消关注' : '关注'} ${item.symbol}`" :disabled="watchBusy" @click="toggleWatch(item)">{{ hasWatch(item) ? '★' : '☆' }}</button></td></tr></tbody></table></div>
              <div v-else class="empty"><span class="empty-icon">◫</span><h3>{{ busy ? '正在读取真实数据…' : snapshot ? '当前条件下暂无交易池' : '行情暂不可用 · UNAVAILABLE' }}</h3><p>{{ snapshot ? '尝试调整筛选条件或切换链。发现范围由来源覆盖决定。' : '请检查数据源状态；页面不会使用样例行情替代真实数据。' }}</p><button v-if="snapshot" class="secondary-button" @click="setPreset('all'); filters.text = ''; filters.label = 'all'; filters.chain = 'all'">重置筛选</button></div>
              <div class="table-footer"><span><i class="status-dot"/> {{ localMode ? '本地真实采样' : '真实公开快照' }}</span><span>按链 + Token + Pool 区分 · FDV 不等同于市值</span></div>
            </section>
            <div class="lower-grid"><section class="panel"><div class="panel-heading"><div><span class="eyebrow">RECENT EVENTS</span><h3>近期风险与关注</h3></div><button class="text-button" @click="navigate('signals')">查看全部 ↗</button></div><div v-if="signals.length" class="recent-signals"><a v-for="signal in signals.slice(0,4)" :key="signal.id" :href="signalHash(signal)"><span class="event-symbol" :class="signalCategory(signal) === 'DATA_FAULT' ? 'data-fault-text' : signalCategory(signal) === 'MARKET_RISK' ? 'risk-text' : 'positive'">{{ signalCategory(signal) === 'DATA_FAULT' ? '⊙' : signalCategory(signal) === 'MARKET_RISK' ? '!' : '↗' }}</span><div><span class="eyebrow">{{ categoryLabel(signal) }} · {{ signal.chain.toUpperCase() }}</span><p>{{ signal.message }}</p><small>{{ shortAddress(signal.token_address) }} · {{ ago(signal.at,now) }}</small></div><span>↗</span></a></div><div v-else class="empty small"><p>暂无已记录的预警事件</p></div></section><section class="panel coverage-panel"><div class="panel-heading"><div><span class="eyebrow">KNOW YOUR DATA</span><h3>理解发现范围</h3></div><span>↗</span></div><p>DEX Screener 的收录与推广，以及 GeckoTerminal 的新池与趋势池，共同提供有限观察样本。</p><div class="coverage-row"><span class="badge boost">BOOSTED</span><span>付费推广标识，不代表自然热度</span></div><div class="coverage-row"><span class="badge neutral">UNKNOWN</span><span>未独立验证蜜罐、持币分布、聪明钱</span></div><div class="coverage-row"><span class="badge neutral">SAMPLED</span><span>采样时间序列，不代表全链完整成交</span></div></section></div>
          </template>

          <ResearchPanel v-if="view === 'research'" :research="research" :local="localMode"/>
          <section v-if="view === 'signals'" class="panel"><div class="panel-heading"><div><span class="eyebrow">OBSERVATION ≠ CONFIRMATION</span><h3>风险、数据故障、关注、确认分开判断</h3></div><span class="badge data-fault">数据故障 {{ dataFaultSignals.length }} 条</span></div><p class="panel-note">首次 6 分钟不发强确认提醒，风险提醒不受延迟影响。数据缺失不会升级状态；点击每条事件展开输入证据、规则版本和因子。来源失败不推断为资金撤出。</p><div v-if="signals.length" class="signal-events"><SignalEvent v-for="signal in signals" :key="signal.id" :signal="signal"/></div><div v-else class="empty"><p>暂无已记录事件；空列表不代表代币安全。</p></div></section>
          <template v-if="view === 'sources'"><LocalHealthPanel :local="localMode" :health="health" :error="healthError"/><section class="panel"><div class="panel-heading"><div><span class="eyebrow">PROVIDER HEALTH</span><h3>实际来源探测结果</h3></div><span class="badge neutral">请求与错误计数来自采集进程</span></div><div v-if="snapshot?.sources.length" class="source-grid"><article v-for="source in snapshot.sources" :key="source.id" class="source-card"><div><h3>{{ source.id }}</h3><span class="badge" :class="source.status === 'OK' ? 'accent' : source.status === 'UNSUPPORTED' ? 'neutral' : 'data-fault'">{{ source.status }}</span></div><p>{{ source.detail }}</p><dl><div><dt>最后检查</dt><dd>{{ utc(source.checked_at) }}</dd></div><div><dt>最后成功</dt><dd>{{ utc(source.last_success_at) }}</dd></div><div><dt>响应耗时</dt><dd>{{ source.latency_ms === null ? '未知' : `${source.latency_ms} ms` }}</dd></div><div><dt>请求 / 错误</dt><dd>{{ source.requests }} / {{ source.errors }}</dd></div><div><dt>连续失败 / 已恢复</dt><dd>{{ sourceHealth(source.id)?.consecutive_failures ?? 'UNKNOWN' }} / {{ sourceHealth(source.id)?.recoveries ?? 'UNKNOWN' }}</dd></div><div><dt>成功数据延迟</dt><dd>{{ source.last_success_at ? ago(source.last_success_at,now) : 'UNKNOWN' }}</dd></div></dl><p v-if="snapshot && (isStale(source.checked_at,snapshot.max_staleness_seconds,now) || (source.status !== 'UNSUPPORTED' && isStale(source.last_success_at,snapshot.max_staleness_seconds,now)))" class="data-fault-text">STALE · 最后检查或成功数据已过期 / 缺失</p></article></div><div v-else class="empty"><p>尚无可验证的来源健康报告。</p></div></section><section v-if="snapshot?.provider_errors.length" class="panel"><div class="panel-heading"><h3>本次来源异常</h3></div><ul class="error-list"><li v-for="problem in snapshot.provider_errors" :key="problem">{{ problem }}</li></ul></section><section class="panel capabilities"><h3>能力边界</h3><p>公开 RPC 仅用于有限链状态查证。Solana 免费 RPC 不支持本产品承诺全网秒级捕获；BSC 官方免费端点的 eth_getLogs 为 UNSUPPORTED。无法连续获得全链 Swap、钱包标签或私有交易数据。</p><p>默认保守请求预算：DEX Screener 发现 ≤8/min、行情 ≤30/min，GeckoTerminal 总计 ≤6/min。服务包含缓存、超时与退避；免费服务仍可能拒绝请求。</p></section></template>
        </template>
        <footer class="page-footer"><span>CHAINFLOW INTELLIGENCE <span class="muted">/</span> 公开数据，可查证的观察。</span><span>只读研究 · 无钱包连接 · 无自动交易</span></footer>
      </div>
    </main>
    <div v-if="notice" class="toast" role="status">{{ notice }}<button aria-label="关闭提示" @click="notice = ''">×</button></div>
  </div>
</template>

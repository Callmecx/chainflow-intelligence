<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ResearchReport, ResearchWindow } from '../types'
import { money, percentage, shortAddress, utc } from '../domain'
import { downloadResearch, missingRate, researchWindows, statusLabel } from '../research'
const props = defineProps<{ research: ResearchReport | null | undefined; local: boolean }>()
const window = ref<ResearchWindow>('1h')
const rule = ref('all')
const rules = computed(()=>[...new Set((props.research?.rule_stats ?? []).map(row=>row.rule))])
const stats = computed(()=>(props.research?.rule_stats ?? []).filter(row=>row.window === window.value && (rule.value === 'all' || row.rule === rule.value)))
const records = computed(()=>(props.research?.records ?? []).filter(row=>rule.value === 'all' || row.rule === rule.value))
const target = (item:{chain:string;token_address:string;pool_address:string})=>`#token/${encodeURIComponent(item.chain)}/${encodeURIComponent(item.token_address)}/${encodeURIComponent(item.pool_address)}`
</script>
<template>
  <section class="panel research-panel">
    <div class="panel-heading"><div><span class="eyebrow">SIGNAL OUTCOMES · SAMPLED</span><h3>真实信号效果与规则对照</h3></div><div v-if="research" class="export-actions"><button class="secondary-button" @click="downloadResearch(research,'csv')">导出 CSV</button><button class="secondary-button" @click="downloadResearch(research,'json')">导出 JSON</button></div></div>
    <p class="panel-note">{{ local ? '使用本机保存的正式信号与真实后续采样。' : '仅展示此次公开作业能够安全公开的研究结果；不访问本地历史或通知配置。' }} 缺价不等于归零，MISSING / CENSORED 不会从样本统计中静默消失。规则评分不是上涨概率。</p>
    <template v-if="research">
      <div class="research-meta"><span class="badge" :class="research.status === 'INSUFFICIENT_SAMPLE' ? 'boost' : 'neutral'">{{ statusLabel(research.status) }}</span><span>每组最低样本 {{ research.min_sample_size }}</span><span>研究生成 {{ utc(research.generated_at_utc) }}</span></div>
      <p class="panel-note">缺失率 =（MISSING + CENSORED）/ 已到期窗口数；PENDING 保留记录，尚未进入到期分母。</p><div class="research-toolbar"><div class="segmented"><button v-for="period in researchWindows" :key="period" :class="{active:window === period}" @click="window = period">{{ period }} 结果</button></div><select v-model="rule" aria-label="研究规则筛选"><option value="all">全部规则</option><option v-for="name in rules" :key="name" :value="name">{{ name }}</option></select></div>
      <div v-if="stats.length" class="table-scroll"><table class="research-table"><thead><tr><th>规则 / 版本</th><th>触发 / 到期 / 有效</th><th>缺失 / 受限 / 等待</th><th>缺失率</th><th>平均 / 中位收益</th><th>负 / 平 / 正样本</th><th>可信度边界</th></tr></thead><tbody><tr v-for="row in stats" :key="`${row.rule}/${row.rule_version}/${row.window}`"><td>{{ row.rule }}<small>{{ row.rule_version }}</small></td><td>{{ row.trigger_count }} / {{ row.eligible_count }} / {{ row.valid_count }}</td><td>{{ row.missing_count }} / {{ row.censored_count }} / {{ row.pending_count }}</td><td>{{ missingRate(row.missing_rate) }}</td><td>{{ percentage(row.mean_return_pct) }}<small>{{ percentage(row.median_return_pct) }}</small></td><td>{{ row.distribution.negative }} / {{ row.distribution.flat }} / {{ row.distribution.positive }}</td><td><span class="badge neutral">{{ statusLabel(row.status) }}</span></td></tr></tbody></table></div>
      <div v-else class="empty small"><p>INSUFFICIENT_SAMPLE · 当前规则暂无可用统计；不会用未来行情补齐。</p></div>
      <div class="panel-heading"><h3>{{ window }} 逐信号研究记录</h3><span class="muted">{{ records.length }} 条正式信号</span></div>
      <div v-if="records.length" class="research-records"><details v-for="record in records" :key="record.signal_id" class="research-record"><summary><span><strong>{{ record.rule }}</strong><small>{{ record.rule_version }} · {{ record.chain.toUpperCase() }} · {{ shortAddress(record.token_address) }}</small></span><span class="badge" :class="record.windows[window]?.status === 'COMPLETE' ? 'accent' : 'neutral'">{{ statusLabel(record.windows[window]?.status) }}</span><span>{{ percentage(record.windows[window]?.return_pct) }}</span></summary><div class="research-record-body"><a :href="target(record)" class="text-button">查看对应交易池 ↗</a><dl><div><dt>Signal ID</dt><dd>{{ record.signal_id }}</dd></div><div><dt>完整 Token / Pool</dt><dd>{{ record.token_address }}<br>{{ record.pool_address }}</dd></div><div><dt>信号触发 / 来源 UTC</dt><dd>{{ utc(record.trigger_at) }}<br>{{ utc(record.source_at) }}</dd></div><div><dt>基准价格 / 流动性</dt><dd>{{ money(record.baseline_price_usd,false) }} / {{ money(record.baseline_liquidity_usd) }}</dd></div><div><dt>目标 / 实际结果 UTC</dt><dd>{{ utc(record.windows[window]?.target_at) }}<br>{{ utc(record.windows[window]?.observed_at) }}</dd></div><div><dt>有效采样 / 最大缺口</dt><dd>{{ record.windows[window]?.samples ?? 'UNKNOWN' }} / {{ record.windows[window]?.max_gap_seconds ?? 'UNKNOWN' }} 秒</dd></div><div><dt>目标时点容差 / 来源改变</dt><dd>{{ record.windows[window]?.tolerance_seconds ?? 'UNKNOWN' }} 秒 / {{ record.windows[window]?.source_changed === true ? '是' : record.windows[window]?.source_changed === false ? '否' : 'UNKNOWN' }}</dd></div><div><dt>最大采样上涨 / 最大回撤</dt><dd>{{ percentage(record.windows[window]?.max_positive_pct) }} / {{ percentage(record.windows[window]?.max_drawdown_pct) }}</dd></div><div><dt>流动性变化 / 是否失效</dt><dd>{{ percentage(record.windows[window]?.liquidity_change_pct) }} / {{ record.windows[window]?.invalidated === true ? '是' : record.windows[window]?.invalidated === false ? '否' : 'UNKNOWN' }}</dd></div></dl><p class="fine-print">{{ record.windows[window]?.reason || 'UNKNOWN · 暂无窗口解释' }}</p></div></details></div>
      <div v-else class="empty small"><p>尚无正式信号研究记录。发现与关注事件不冒充正式确认样本。</p></div>
      <ul class="research-limitations"><li v-for="(limitation,index) in research.limitations" :key="index">{{ limitation }}</li><li>收益及极值仅来自记录中的实际采样；不覆盖采样间价格路径，不包含滑点、手续费或成交保证，不据此宣称稳定盈利。</li></ul>
    </template>
    <div v-else class="empty"><strong>UNAVAILABLE · 当前数据没有 V1.1 研究报告</strong><p>旧版快照或尚未完成采集时，此处保持未知。已有代币行情仍可独立使用。</p></div>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Signal } from '../types'
import { isAddress, money, utc } from '../domain'
import { categoryClass, categoryLabel, publicEvidence } from '../research'
const props = defineProps<{ signal: Signal }>()
const evidence = computed(()=>publicEvidence(props.signal.evidence_snapshot))
const target = computed(()=>isAddress(props.signal.chain,props.signal.token_address) ? `#token/${encodeURIComponent(props.signal.chain)}/${encodeURIComponent(props.signal.token_address)}/${encodeURIComponent(props.signal.pool_address)}` : '#sources')
</script>
<template>
  <article class="signal-event">
    <details :aria-label="`信号证据 ${signal.id}`">
      <summary>
        <span class="signal-meta"><span class="badge" :class="categoryClass(signal)">{{ categoryLabel(signal) }}</span><span>{{ signal.state }} · {{ signal.rule }}</span><time>{{ utc(signal.at) }}</time></span>
        <strong>{{ signal.message }}</strong><span class="expand-evidence">查看输入证据与规则 ↓</span>
      </summary>
      <div class="signal-detail">
        <a :href="target" class="text-button">{{ target === '#sources' ? '查看数据源健康' : '查看对应代币池' }} ↗</a>
        <dl class="signal-identifiers"><div><dt>Signal ID / 规则版本</dt><dd>{{ signal.id }} / {{ signal.rule_version || 'UNKNOWN · V1 历史事件' }}</dd></div><div><dt>链 / 完整 Token</dt><dd>{{ signal.chain }} / {{ signal.token_address }}</dd></div><div><dt>完整 Pool</dt><dd>{{ signal.pool_address || '不适用' }}</dd></div><div><dt>触发 UTC</dt><dd>{{ utc(signal.at) }}</dd></div><div><dt>来源数据 UTC</dt><dd>{{ utc(signal.source_at) }}</dd></div><div><dt>触发基准价格 / 流动性</dt><dd>{{ money(signal.baseline_price_usd,false) }} / {{ money(signal.baseline_liquidity_usd) }}</dd></div></dl>
        <h4>规则输入与触发原因</h4><ul v-if="signal.evidence.length"><li v-for="(item,index) in signal.evidence" :key="index">{{ item }}</li></ul><p v-else class="muted">UNKNOWN · 尚无证据记录。</p>
        <div v-if="signal.factors?.length" class="factor-list"><h4>可解释因子 · 分数不是上涨概率</h4><article v-for="(factor,index) in signal.factors" :key="index"><strong>{{ factor.name }}</strong><span>实际值 {{ factor.value ?? 'UNKNOWN' }} · 权重 {{ factor.weight ?? 'UNKNOWN' }} · 分数 {{ factor.score ?? 'UNKNOWN' }}</span><p v-for="(reason,index2) in factor.evidence" :key="index2">{{ reason }}</p></article></div>
        <details v-if="Object.keys(evidence).length" class="raw-evidence"><summary>触发时公开行情快照</summary><dl><div v-for="(value,key) in evidence" :key="key"><dt>{{ key }}</dt><dd>{{ value ?? 'UNKNOWN' }}</dd></div></dl></details>
        <p class="fine-print">以上输入属于事件发生时的记录。当前行情变化不会覆盖触发证据；数据故障不等于代币流动性撤出。</p>
      </div>
    </details>
  </article>
</template>

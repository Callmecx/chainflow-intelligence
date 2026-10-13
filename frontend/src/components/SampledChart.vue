<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Point } from '../types'
import { chartSegments, money, utc } from '../domain'
const props = defineProps<{ history: Point[] }>()
const field = ref<'price_usd' | 'liquidity_usd'>('price_usd')
const chart = computed(() => chartSegments(props.history, field.value))
</script>
<template>
  <section class="panel chart-panel">
    <div class="panel-heading"><div><span class="eyebrow">真实采样 · SAMPLED</span><h3>观察轨迹</h3></div><div class="segmented compact"><button :class="{active:field === 'price_usd'}" @click="field = 'price_usd'">价格</button><button :class="{active:field === 'liquidity_usd'}" @click="field = 'liquidity_usd'">流动性</button></div></div>
    <template v-if="chart"><div class="chart-range"><span>{{ money(chart.max, false) }}</span><span>{{ chart.count }} 个真实采样点</span></div><svg viewBox="0 0 800 200" role="img" :aria-label="`真实${field === 'price_usd' ? '价格' : '流动性'}采样走势`" class="sample-chart"><path d="M16 24H784 M16 100H784 M16 176H784" class="chart-grid"/><polyline v-for="(path,index) in chart.paths" :key="index" :points="path" class="chart-line"/></svg><div class="chart-range"><span>{{ money(chart.min, false) }}</span><span>{{ utc(chart.start) }} — {{ utc(chart.end) }}</span></div></template>
    <div v-else class="empty chart-empty"><span class="empty-icon">⌁</span><strong>尚无足量真实采样</strong><p>至少需要两个不同时间的有效采样点。持续运行本地服务后累积轨迹。</p></div>
    <p class="fine-print">采样连线用于观察，不是完整 K 线或逐笔成交。缺失值断开显示，采样之间的价格路径未知。</p>
  </section>
</template>

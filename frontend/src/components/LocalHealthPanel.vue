<script setup lang="ts">
import { computed } from 'vue'
import type { LocalHealth } from '../types'
import { utc } from '../domain'
const props = defineProps<{ local: boolean; health: LocalHealth | null; error: string }>()
const telegram = computed(()=>props.local ? props.health?.telegram : undefined)
const queue = computed(()=>(telegram.value?.queue ?? {}) as Record<string,number>)
const queueKnown = computed(()=>telegram.value?.queue !== null && typeof telegram.value?.queue === 'object')
const queueStates = ['PENDING','RETRY','SENDING','SENT','UNCERTAIN','FAILED','EXPIRED','SUPPRESSED']
const delivery = computed(()=>(telegram.value?.last_delivery ?? null) as {state?:string;code?:string;at_unix?:number} | null)
const bool = (value:unknown)=>value === true ? '是' : value === false ? '否' : 'UNKNOWN'
const deliveryTime = computed(()=>typeof delivery.value?.at_unix === 'number' ? utc(new Date(delivery.value.at_unix*1000).toISOString()) : 'UNKNOWN')
</script>
<template>
  <section v-if="!local" class="panel public-privacy"><h3>公开站的健康信息边界</h3><p>此页仅展示公开采集来源。系统启动时间、本地数据库、私人监控名单和 Telegram 配置只在本地看板读取；公开站不会请求本地监控接口。</p></section>
  <section v-else class="panel local-health"><div class="panel-heading"><div><span class="eyebrow">LOCAL MONITOR HEALTH</span><h3>本地监控与通知健康</h3></div><span class="badge" :class="health?.status === 'OK' ? 'accent' : 'data-fault'">{{ error ? 'UNAVAILABLE' : health?.status || 'UNKNOWN' }}</span></div><p v-if="error" class="panel-note data-fault-text">{{ error }}</p><div v-if="health" class="health-grid"><article><h4>采集进程与 SQLite</h4><dl><div><dt>系统启动 UTC</dt><dd>{{ utc(health.started_at) }}</dd></div><div><dt>最近扫描 UTC</dt><dd>{{ utc(health.last_scan_at) }}</dd></div><div><dt>数据库一致性 / 日志模式</dt><dd>{{ health.database?.integrity_check ?? 'UNKNOWN' }} / {{ health.database?.journal_mode ?? 'UNKNOWN' }}</dd></div><div><dt>扫描轮次 / 已接收采样</dt><dd>{{ health.runtime?.scans ?? 'UNKNOWN' }} / {{ health.runtime?.accepted_samples ?? 'UNKNOWN' }}</dd></div><div><dt>来源错误 / 最近扫描耗时</dt><dd>{{ health.runtime?.provider_errors ?? 'UNKNOWN' }} / {{ health.runtime?.last_scan_duration_ms ?? 'UNKNOWN' }} ms</dd></div></dl></article><article><h4>Telegram 安全配置状态</h4><dl><div><dt>凭据已配置 / 通知启用</dt><dd>{{ bool(telegram?.configured) }} / {{ bool(telegram?.enabled) }}</dd></div><div><dt>真实投递验证</dt><dd>{{ telegram?.verification === 'DELIVERED_RECORDED' ? 'DELIVERED_RECORDED' : 'PENDING' }}</dd></div><div><dt>最近投递状态 / 分类</dt><dd>{{ delivery?.state ?? 'UNKNOWN' }} / {{ delivery?.code ?? 'UNKNOWN' }}</dd></div><div><dt>最近投递 UTC</dt><dd>{{ deliveryTime }}</dd></div></dl><p class="fine-print">未配置时监控照常运行。PENDING 表示尚无成功响应记录；Mock 测试不代表真实 Telegram 已发送。通知只在本机环境变量中配置。</p></article></div><div v-if="queueKnown" class="queue-status"><span v-for="state in queueStates" :key="state"><b>{{ state }}</b>{{ queue[state] ?? 0 }}</span></div><p v-if="telegram" class="panel-note">UNCERTAIN 表示请求可能已送达，默认不自动重发。发送与记录无法保证绝对 Exactly Once；界面不接收或显示 Token、Chat ID。</p><div v-if="!health && !error" class="empty small"><p>尚无本地健康报告。</p></div></section>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { GraphRow } from '../types'
import { theme } from '../store'

const props = defineProps<{ rows: GraphRow[]; selected: string }>()

/** 读一个 px 数值的 CSS token；取不到就用兜底值。 */
function readPx(name: string, fallback: number): number {
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  const n = Number.parseFloat(raw)
  return Number.isFinite(n) && n > 0 ? n : fallback
}

/**
 * 行高必须与 DOM 列表的行高一致（`style.css` 的 `--graph-row`），否则 SVG 泳道
 * 与提交行会逐行错位。此前这里是硬编码的 22，与 CSS 里的 `height: 22px` 跨语言
 * 耦合、只靠注释维系——现在两边都从同一个 token 取值。
 */
const ROW = readPx('--graph-row', 22)
const COL = 14
const PAD = 10

const FALLBACK_LANES = ['#3ddc84', '#5aa9e6', '#e0b341', '#c586c0', '#e06c75', '#56b6c2']

// 泳道色来自主题 token，换主题时提交图跟着变；不依赖 CSS 变量解析失败时的兜底会掉回默认配色
const lanes = computed(() => {
  void theme.value
  const cs = getComputedStyle(document.documentElement)
  return FALLBACK_LANES.map((fallback, i) => cs.getPropertyValue(`--lane-${i + 1}`).trim() || fallback)
})

const width = computed(() => {
  let maxLane = 0
  for (const r of props.rows) {
    maxLane = Math.max(maxLane, r.lane)
    for (const p of r.passes) maxLane = Math.max(maxLane, p)
    for (const e of r.edges) maxLane = Math.max(maxLane, e.toLane)
  }
  return PAD * 2 + (maxLane + 1) * COL
})
const height = computed(() => props.rows.length * ROW)

function x(lane: number) {
  return PAD + lane * COL
}
function y(i: number) {
  return i * ROW + ROW / 2
}
function stroke(lane: number) {
  const colors = lanes.value
  return colors[lane % colors.length]
}
</script>

<template>
  <svg class="git-graph" :width="width" :height="height">
    <template v-for="(r, i) in rows" :key="r.commit.hash">
      <!-- 贯穿竖线 -->
      <line
        v-for="lane in r.passes"
        :key="`p-${i}-${lane}`"
        :x1="x(lane)"
        :x2="x(lane)"
        :y1="y(i) - ROW / 2"
        :y2="y(i) + ROW / 2"
        :stroke="stroke(lane)"
        stroke-width="1.5"
      />
      <!-- 到额外父提交的斜线 -->
      <line
        v-for="(e, ei) in r.edges"
        :key="`e-${i}-${ei}`"
        :x1="x(r.lane)"
        :y1="y(i)"
        :x2="x(e.toLane)"
        :y2="y(i) + ROW"
        :stroke="stroke(e.toLane)"
        stroke-width="1.5"
      />
      <circle
        v-if="r.commit.parents.length > 1"
        :cx="x(r.lane)"
        :cy="y(i)"
        :r="selected === r.commit.hash ? 5 : 4"
        fill="var(--panel)"
        :stroke="stroke(r.color)"
        stroke-width="2"
      >
        <title>{{ r.commit.short }} · merge · {{ r.commit.subject }} — {{ r.commit.author }}</title>
      </circle>
      <circle
        v-else
        :cx="x(r.lane)"
        :cy="y(i)"
        :r="selected === r.commit.hash ? 4.5 : 3.5"
        :fill="stroke(r.color)"
      >
        <title>{{ r.commit.short }} · {{ r.commit.subject }} — {{ r.commit.author }}</title>
      </circle>
    </template>
  </svg>
</template>

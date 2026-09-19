<script setup lang="ts">
import { computed, ref } from 'vue'
import type { DiffLine, FileDiff, Hunk } from '../types'

const props = withDefaults(
  defineProps<{
    file?: FileDiff | null
    loading?: boolean
    ignoreWhitespace?: boolean
    fullContext?: boolean
  }>(),
  { file: null, loading: false, ignoreWhitespace: false, fullContext: false },
)
const emit = defineEmits<{ 'toggle-whitespace': []; 'expand-all': [] }>()

const split = ref(false)

interface SplitRow {
  left?: DiffLine
  right?: DiffLine
}

function splitRows(hunk: Hunk): SplitRow[] {
  const rows: SplitRow[] = []
  const lines = hunk.lines
  let i = 0
  while (i < lines.length) {
    const l = lines[i]
    if (l.kind === 'context') {
      rows.push({ left: l, right: l })
      i++
      continue
    }
    const dels: DiffLine[] = []
    while (i < lines.length && lines[i].kind === 'del') dels.push(lines[i++])
    const adds: DiffLine[] = []
    while (i < lines.length && lines[i].kind === 'add') adds.push(lines[i++])
    const n = Math.max(dels.length, adds.length)
    for (let k = 0; k < n; k++) rows.push({ left: dels[k], right: adds[k] })
  }
  return rows
}

const add = computed(() => props.file?.additions ?? 0)
const del = computed(() => props.file?.deletions ?? 0)
</script>

<template>
  <div class="gc-body git-diff-body">
    <div v-if="loading" class="gc-empty">加载中…</div>
    <template v-else-if="file">
      <div class="diff-toolbar mono">
        <span class="diff-stat-add">+{{ add }}</span>
        <span class="diff-stat-del">−{{ del }}</span>
        <span class="v-spacer" />
        <button class="ghost" :class="{ on: ignoreWhitespace }" @click="emit('toggle-whitespace')">隐藏空白</button>
        <button class="ghost" :class="{ on: fullContext }" @click="emit('expand-all')">{{ fullContext ? '收起全部' : '展开全部' }}</button>
        <button class="ghost" :class="{ on: split }" @click="split = !split">{{ split ? '统一' : '左右' }}</button>
      </div>
      <div v-if="file.binary" class="gc-empty">二进制文件，无法显示差异</div>
      <div v-else-if="file.hunks.length === 0" class="gc-empty">无差异</div>
      <div v-else class="diff-body">
        <div v-for="(h, hi) in file.hunks" :key="hi" class="diff-hunk">
          <div class="diff-hunk-head mono">{{ h.header }}</div>
          <template v-if="!split">
            <div v-for="(l, li) in h.lines" :key="li" class="diff-line" :class="`k-${l.kind}`">
              <span class="dl-no mono">{{ l.oldNo ?? '' }}</span>
              <span class="dl-no mono">{{ l.newNo ?? '' }}</span>
              <span class="dl-mark mono">{{ l.kind === 'add' ? '+' : l.kind === 'del' ? '-' : ' ' }}</span>
              <span class="dl-text mono">{{ l.text }}</span>
            </div>
          </template>
          <template v-else>
            <div v-for="(r, ri) in splitRows(h)" :key="ri" class="diff-split-row">
              <div class="diff-line half" :class="r.left ? `k-${r.left.kind}` : 'k-empty'">
                <span class="dl-no mono">{{ r.left?.oldNo ?? '' }}</span>
                <span class="dl-text mono">{{ r.left?.text ?? '' }}</span>
              </div>
              <div class="diff-line half" :class="r.right ? `k-${r.right.kind}` : 'k-empty'">
                <span class="dl-no mono">{{ r.right?.newNo ?? '' }}</span>
                <span class="dl-text mono">{{ r.right?.text ?? '' }}</span>
              </div>
            </div>
          </template>
        </div>
        <div v-if="file.truncated" class="gt-truncated">内容过大，未完整渲染（可点「展开全部」重取）</div>
      </div>
    </template>
    <div v-else class="gc-empty">选择一条提交或文件查看差异</div>
  </div>
</template>

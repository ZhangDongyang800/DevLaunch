<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { gitBinaryPreview } from '../api'
import type { BinaryPreview, DiffLine, FileDiff, Hunk } from '../types'
import GitImageDiff from './GitImageDiff.vue'

const props = withDefaults(
  defineProps<{
    file?: FileDiff | null
    loading?: boolean
    ignoreWhitespace?: boolean
    fullContext?: boolean
    projectId?: string
    hash?: string
  }>(),
  { file: null, loading: false, ignoreWhitespace: false, fullContext: false, projectId: '', hash: '' },
)
const emit = defineEmits<{ 'toggle-whitespace': []; 'expand-all': [] }>()

const split = ref(false)

// 二进制侧内容按需取（一次点击一次请求），文本 diff 不触发。
const preview = ref<BinaryPreview | null>(null)
const pvLoading = ref(false)
const pvError = ref('')
let pvSeq = 0

async function loadPreview() {
  const seq = ++pvSeq
  preview.value = null
  pvError.value = ''
  pvLoading.value = false
  if (!props.file?.binary || !props.projectId) return
  pvLoading.value = true
  try {
    const got = await gitBinaryPreview(props.projectId, props.file.path, props.file.staged, props.hash || undefined)
    if (seq !== pvSeq) return
    preview.value = got
  } catch (e) {
    if (seq !== pvSeq) return
    pvError.value = `${e}`
  } finally {
    if (seq === pvSeq) pvLoading.value = false
  }
}

watch(
  () => [props.projectId, props.hash, props.file?.binary, props.file?.path, props.file?.staged],
  loadPreview,
  { immediate: true },
)

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
      <template v-if="!file.binary">
        <div class="diff-toolbar mono">
          <span class="diff-stat-add">+{{ add }}</span>
          <span class="diff-stat-del">−{{ del }}</span>
          <span class="v-spacer" />
          <button class="ghost" :class="{ on: ignoreWhitespace }" @click="emit('toggle-whitespace')">隐藏空白</button>
          <button class="ghost" :class="{ on: fullContext }" @click="emit('expand-all')">{{ fullContext ? '收起全部' : '展开全部' }}</button>
          <button class="ghost" :class="{ on: split }" @click="split = !split">{{ split ? '统一' : '左右' }}</button>
        </div>
        <div v-if="file.hunks.length === 0" class="gc-empty">无差异</div>
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
      <GitImageDiff v-else :preview="preview" :loading="pvLoading" :error="pvError" />
    </template>
    <div v-else class="gc-empty">选择一条提交或文件查看差异</div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { gitCommitDetail } from '../api'
import type { CommitDetail } from '../types'
import { loadLog, logCache } from '../gitStore'
import GitGraph from './GitGraph.vue'
import GitDiff from './GitDiff.vue'

const props = defineProps<{ projectId: string }>()

const rows = computed(() => logCache.value[props.projectId] ?? [])
const selected = ref('')
const detail = ref<CommitDetail | null>(null)
const loading = ref(false)
const error = ref('')
let seq = 0
let loadingMore = false
let exhausted = false

onMounted(async () => {
  try {
    await loadLog(props.projectId, true)
  } catch (e) {
    error.value = `${e}`
  }
})

async function onScroll(e: Event) {
  const el = e.target as HTMLElement
  if (loadingMore || exhausted) return
  if (el.scrollTop + el.clientHeight < el.scrollHeight - 40) return
  const before = rows.value.length
  if (before === 0) return
  loadingMore = true
  try {
    await loadLog(props.projectId, false)
    if (rows.value.length === before) exhausted = true
  } catch (e) {
    error.value = `${e}`
  } finally {
    loadingMore = false
  }
}

async function select(hash: string) {
  const mine = ++seq
  selected.value = hash
  loading.value = true
  error.value = ''
  detail.value = null
  try {
    const d = await gitCommitDetail(props.projectId, hash)
    if (mine !== seq) return
    detail.value = d
  } catch (e) {
    if (mine !== seq) return
    error.value = `${e}`
  } finally {
    if (mine === seq) loading.value = false
  }
}

function relTime(iso: string): string {
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return iso
  const d = Date.now() - t
  const day = 86_400_000
  if (d < 60_000) return '刚刚'
  if (d < 3_600_000) return `${Math.floor(d / 60_000)} 分钟前`
  if (d < day) return `${Math.floor(d / 3_600_000)} 小时前`
  if (d < day * 30) return `${Math.floor(d / day)} 天前`
  return new Date(t).toLocaleDateString()
}
</script>

<template>
  <div class="git-history">
    <div class="git-history-log">
      <div class="git-col git-commits">
        <div class="gc-body git-log-scroll" @scroll="onScroll">
          <GitGraph :rows="rows" :selected="selected" />
          <div class="git-log-list">
            <div
              v-for="r in rows"
              :key="r.commit.hash"
              class="gt-commit"
              :class="{ active: selected === r.commit.hash }"
              @click="select(r.commit.hash)"
            >
              <span class="gt-hash mono">{{ r.commit.short }}</span>
              <span class="gt-subject">{{ r.commit.subject }}</span>
              <span class="gt-author">{{ r.commit.author }}</span>
              <span class="gt-time">{{ relTime(r.commit.date) }}</span>
            </div>
            <div v-if="rows.length === 0" class="gc-empty">还没有提交</div>
          </div>
        </div>
      </div>
    </div>
    <div class="git-col git-diff">
      <div class="gc-head">差异<span v-if="error" class="gp-error"> · {{ error }}</span></div>
      <GitDiff :stat="detail?.stat" :text="detail?.patch" :truncated="detail?.truncated" :loading="loading" />
    </div>
  </div>
</template>

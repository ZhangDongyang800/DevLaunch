<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { gitCommit } from '../api'
import type { CommitDetail } from '../types'
import { loadLog, logCache, statuses } from '../gitStore'
import GitGraph from './GitGraph.vue'

const props = defineProps<{ projectId: string }>()

const status = computed(() => statuses.value[props.projectId])
const rows = computed(() => logCache.value[props.projectId] ?? [])
const selected = ref('')
const detail = ref<CommitDetail | null>(null)
const error = ref('')
const loadingDetail = ref(false)
let loadingMore = false
let exhausted = false

onMounted(async () => {
  if (rows.value.length > 0) return
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
  selected.value = hash
  loadingDetail.value = true
  error.value = ''
  detail.value = null
  try {
    detail.value = await gitCommit(props.projectId, hash)
  } catch (e) {
    error.value = `${e}`
  } finally {
    loadingDetail.value = false
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
  <div class="git-panel">
    <div class="git-panel-head">
      <span class="gp-title mono">{{ status?.detached ? 'detached' : status?.branch || '—' }}</span>
      <span v-if="status" class="gp-counts mono">
        {{ status.staged }} staged · {{ status.unstaged }} unstaged · {{ status.untracked }} untracked
      </span>
      <span class="spacer" />
      <span v-if="error" class="gp-error">{{ error }}</span>
    </div>

    <div class="git-columns">
      <div class="git-col git-worktree">
        <div class="gc-head">工作树 ({{ status?.files.length ?? 0 }})</div>
        <div class="gc-body">
          <div v-for="f in status?.files ?? []" :key="f.index + f.worktree + f.path" class="gt-file" :title="f.path">
            <span class="gt-status mono">{{ f.index !== '.' ? f.index : f.worktree }}</span>
            <span class="gt-path mono">{{ f.path }}</span>
            <span class="gt-label">{{ f.status }}</span>
          </div>
          <div v-if="(status?.files.length ?? 0) === 0" class="gc-empty">工作树干净</div>
        </div>
      </div>

      <div class="git-col git-commits">
        <div class="gc-head">提交</div>
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

      <div class="git-col git-diff">
        <div class="gc-head">差异</div>
        <div class="gc-body">
          <pre v-if="detail" class="git-patch mono">{{ detail.stat }}{{ detail.patch }}</pre>
          <div v-else-if="loadingDetail" class="gc-empty">加载中…</div>
          <div v-else class="gc-empty">选择一条提交查看差异</div>
          <div v-if="detail?.truncated" class="gt-truncated">diff 过大，未完整渲染</div>
        </div>
      </div>
    </div>
  </div>
</template>

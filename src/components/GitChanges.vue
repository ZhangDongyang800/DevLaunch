<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { gitFileDiff } from '../api'
import type { FileChange, FileDiff } from '../types'
import { statuses } from '../gitStore'
import GitDiff from './GitDiff.vue'

const props = defineProps<{ projectId: string }>()

const status = computed(() => statuses.value[props.projectId])
const files = computed<FileChange[]>(() => status.value?.files ?? [])
const staged = computed(() =>
  files.value.filter((f) => f.index !== '.' && f.index !== '?' && f.index !== '!' && f.index !== ' '),
)
const unstaged = computed(() =>
  files.value.filter((f) => f.worktree !== '.' && f.worktree !== '?' && f.worktree !== '!' && f.worktree !== ' '),
)
const untracked = computed(() => files.value.filter((f) => f.index === '?'))

const selected = ref('')
const diff = ref<FileDiff | null>(null)
const loading = ref(false)
const error = ref('')

watch(
  () => props.projectId,
  () => {
    selected.value = ''
    diff.value = null
  },
)

async function open(path: string, isStaged: boolean) {
  const key = `${isStaged ? 's' : 'w'}:${path}`
  selected.value = key
  loading.value = true
  error.value = ''
  diff.value = null
  try {
    diff.value = await gitFileDiff(props.projectId, path, isStaged)
  } catch (e) {
    error.value = `${e}`
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="git-changes">
    <div class="git-changes-lists">
      <div class="git-col">
        <div class="gc-head">已暂存 ({{ staged.length }})</div>
        <div class="gc-body">
          <div
            v-for="f in staged"
            :key="'s' + f.path"
            class="gt-file clickable"
            :class="{ active: selected === 's:' + f.path }"
            :title="f.path"
            @click="open(f.path, true)"
          >
            <span class="gt-status mono">{{ f.index }}</span>
            <span class="gt-path mono">{{ f.path }}</span>
            <span class="gt-label">{{ f.status }}</span>
          </div>
          <div v-if="staged.length === 0" class="gc-empty">无已暂存改动</div>
        </div>
      </div>

      <div class="git-col">
        <div class="gc-head">未暂存 / 未跟踪 ({{ unstaged.length + untracked.length }})</div>
        <div class="gc-body">
          <div
            v-for="f in unstaged"
            :key="'w' + f.path"
            class="gt-file clickable"
            :class="{ active: selected === 'w:' + f.path }"
            :title="f.path"
            @click="open(f.path, false)"
          >
            <span class="gt-status mono">{{ f.worktree }}</span>
            <span class="gt-path mono">{{ f.path }}</span>
            <span class="gt-label">{{ f.status }}</span>
          </div>
          <div
            v-for="f in untracked"
            :key="'u' + f.path"
            class="gt-file clickable"
            :class="{ active: selected === 'w:' + f.path }"
            :title="f.path"
            @click="open(f.path, false)"
          >
            <span class="gt-status mono">?</span>
            <span class="gt-path mono">{{ f.path }}</span>
            <span class="gt-label">未跟踪</span>
          </div>
          <div v-if="unstaged.length + untracked.length === 0" class="gc-empty">工作树干净</div>
        </div>
      </div>
    </div>

    <div class="git-col git-diff">
      <div class="gc-head">
        差异<span v-if="diff?.untracked"> · 新文件</span><span v-if="error" class="gp-error"> · {{ error }}</span>
      </div>
      <GitDiff :text="diff?.text" :truncated="diff?.truncated" :untracked="diff?.untracked" :loading="loading" />
    </div>
  </div>
</template>

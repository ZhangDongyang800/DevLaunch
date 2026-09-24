<script setup lang="ts">
import { computed } from 'vue'
import type { RepoStatus } from '../types'
import { classifyGitStatusError } from '../utils'

const props = defineProps<{ status?: RepoStatus }>()
const emit = defineEmits<{ open: [] }>()

const dirty = computed(() => {
  const s = props.status
  return s ? s.staged + s.unstaged + s.untracked : 0
})

const errorInfo = computed(() => classifyGitStatusError(props.status?.error))

const dirtyTitle = computed(() => {
  const s = props.status
  if (!s) return ''
  // 截断时必须说出来：否则"少列了几个文件"会被当成"就这些"。
  const base = `${s.staged} staged / ${s.unstaged} unstaged / ${s.untracked} untracked`
  return s.truncated ? `${base}\n（git status 输出超限被截断，以上数字不完整）` : base
})
</script>

<template>
  <button
    v-if="status && status.isRepo"
    type="button"
    class="git-badge mono"
    title="在 Git 页查看"
    @click.stop="emit('open')"
  >
    <span class="gb-branch">{{ status.detached ? 'detached' : status.branch }}</span>
    <span v-if="dirty" class="gb-dirty" :title="dirtyTitle">●{{ dirty }}</span>
    <span v-if="status.truncated" class="gb-warn" title="git status 输出超限被截断，统计不完整">截断</span>
    <span v-if="status.ahead" class="gb-ahead">↑{{ status.ahead }}</span>
    <span v-if="status.behind" class="gb-behind">↓{{ status.behind }}</span>
  </button>
  <span
    v-else-if="status && status.error"
    class="gb-warn"
    :class="{ 'gb-git': errorInfo.kind === 'git', 'gb-root': errorInfo.kind === 'root' }"
    :title="status.error"
  >
    {{ errorInfo.kind === 'root' ? '根目录不可用' : errorInfo.kind === 'git' ? 'Git 未找到' : '—' }}
  </span>
</template>

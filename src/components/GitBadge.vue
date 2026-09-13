<script setup lang="ts">
import { computed } from 'vue'
import type { RepoStatus } from '../types'

const props = defineProps<{ status?: RepoStatus; expanded: boolean }>()
const emit = defineEmits<{ toggle: []; refresh: [] }>()

const dirty = computed(() => {
  const s = props.status
  return s ? s.staged + s.unstaged + s.untracked : 0
})

const dirtyTitle = computed(() => {
  const s = props.status
  return s ? `${s.staged} staged / ${s.unstaged} unstaged / ${s.untracked} untracked` : ''
})
</script>

<template>
  <span v-if="status && status.isRepo" class="git-badge mono" :class="{ open: expanded }">
    <button
      type="button"
      class="git-badge-toggle"
      :aria-expanded="expanded"
      :title="expanded ? '收起 Git 概览' : '展开 Git 概览'"
      @click.stop="emit('toggle')"
    >
      <span class="gb-caret">{{ expanded ? '▾' : '▸' }}</span>
      <span class="gb-branch">{{ status.detached ? 'detached' : status.branch }}</span>
      <span v-if="dirty" class="gb-dirty" :title="dirtyTitle">●{{ dirty }}</span>
      <span v-if="status.ahead" class="gb-ahead">↑{{ status.ahead }}</span>
      <span v-if="status.behind" class="gb-behind">↓{{ status.behind }}</span>
    </button>
    <button type="button" class="gb-refresh" title="刷新 Git 状态" @click.stop="emit('refresh')">⟳</button>
  </span>
  <span v-else-if="status && status.error" class="gb-warn" :title="status.error">—</span>
</template>

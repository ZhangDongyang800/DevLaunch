<script setup lang="ts">
import { computed } from 'vue'
import type { RepoStatus } from '../types'

const props = defineProps<{ status?: RepoStatus }>()

const dirty = computed(() => {
  const s = props.status
  return s ? s.staged + s.unstaged + s.untracked : 0
})
</script>

<template>
  <span v-if="status && status.isRepo" class="git-badge mono">
    <span class="gb-branch">{{ status.detached ? 'detached' : status.branch }}</span>
    <span v-if="dirty" class="gb-dirty" :title="`${status.staged} staged / ${status.unstaged} unstaged / ${status.untracked} untracked`">●{{ dirty }}</span>
    <span v-if="status.ahead" class="gb-ahead">↑{{ status.ahead }}</span>
    <span v-if="status.behind" class="gb-behind">↓{{ status.behind }}</span>
  </span>
  <span v-else-if="status && status.error" class="gb-warn" :title="status.error">—</span>
</template>

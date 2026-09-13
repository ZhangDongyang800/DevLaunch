<script setup lang="ts">
import { computed } from 'vue'
import type { RepoStatus } from '../types'

const props = defineProps<{ status?: RepoStatus }>()
const emit = defineEmits<{ open: [] }>()

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
  <button
    v-if="status && status.isRepo"
    type="button"
    class="git-badge mono"
    title="在 Git 页查看"
    @click.stop="emit('open')"
  >
    <span class="gb-branch">{{ status.detached ? 'detached' : status.branch }}</span>
    <span v-if="dirty" class="gb-dirty" :title="dirtyTitle">●{{ dirty }}</span>
    <span v-if="status.ahead" class="gb-ahead">↑{{ status.ahead }}</span>
    <span v-if="status.behind" class="gb-behind">↓{{ status.behind }}</span>
  </button>
  <span v-else-if="status && status.error" class="gb-warn" :title="status.error">—</span>
</template>

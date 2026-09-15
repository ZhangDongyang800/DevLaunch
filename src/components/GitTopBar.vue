<script setup lang="ts">
import { computed } from 'vue'
import type { Project, RepoStatus } from '../types'
import { branches, refreshRepo, refreshStatuses, selectedRepoId, statuses } from '../gitStore'

const props = defineProps<{ projects: Project[] }>()

const selected = computed(() => props.projects.find((p) => p.id === selectedRepoId.value) ?? null)
const status = computed<RepoStatus | undefined>(() => (selected.value ? statuses.value[selected.value.id] : undefined))
const currentBranch = computed(() => {
  const list = branches.value[selectedRepoId.value] ?? []
  return list.find((b) => b.current)?.name ?? status.value?.branch ?? '—'
})
const operation = computed(() => status.value?.operation ?? '')

function onRepoChange(e: Event) {
  selectedRepoId.value = (e.target as HTMLSelectElement).value
  void refreshRepo(selectedRepoId.value)
}

function dirty(p: Project) {
  const s = statuses.value[p.id]
  return s ? s.staged + s.unstaged + s.untracked : 0
}

async function refreshAll() {
  await refreshStatuses(props.projects.map((p) => p.id))
  if (selectedRepoId.value) await refreshRepo(selectedRepoId.value)
}
</script>

<template>
  <div class="git-topbar">
    <label class="gtb-field">
      <span class="gtb-label">仓库</span>
      <select :value="selectedRepoId" @change="onRepoChange">
        <option v-for="p in projects" :key="p.id" :value="p.id">
          {{ p.name || '未命名项目' }}{{ dirty(p) ? ` ●${dirty(p)}` : '' }}
        </option>
      </select>
    </label>
    <div class="gtb-field">
      <span class="gtb-label">分支</span>
      <span class="gtb-branch mono">{{ currentBranch }}</span>
    </div>
    <span class="spacer" />
    <button class="bordered" @click="refreshAll">刷新</button>
  </div>

  <div v-if="operation" class="git-op-banner">
    存在未完成的 {{ operation }}，请在终端处理后再进行提交 / 分支操作。
  </div>
</template>

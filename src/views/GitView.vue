<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, watch } from 'vue'
import { config } from '../store'
import { branches, gitError, loadLastFetch, refreshRepo, refreshStatuses, selectedRepoId, statuses, tab } from '../gitStore'
import GitTopBar from '../components/GitTopBar.vue'
import GitChanges from '../components/GitChanges.vue'
import GitHistory from '../components/GitHistory.vue'

const props = defineProps<{ projectId?: string }>()

const projects = computed(() => config.value?.projects ?? [])
const selected = computed(() => projects.value.find((p) => p.id === selectedRepoId.value) ?? null)
const selectedStatus = computed(() => (selected.value ? statuses.value[selected.value.id] : undefined))

onMounted(async () => {
  await refreshStatuses(projects.value.map((p) => p.id))
  const start =
    (props.projectId && projects.value.some((p) => p.id === props.projectId) ? props.projectId : '') ||
    projects.value.find((p) => statuses.value[p.id]?.isRepo)?.id ||
    projects.value[0]?.id ||
    ''
  selectedRepoId.value = start
  if (start) void refreshRepo(start)
})

watch(
  () => props.projectId,
  (id) => {
    if (id) {
      selectedRepoId.value = id
      void refreshRepo(id)
    }
  },
)

function onKey(e: KeyboardEvent) {
  if (!e.ctrlKey) return
  const k = e.key.toLowerCase()
  if (k === 'r') {
    e.preventDefault()
    if (selectedRepoId.value) {
      void refreshRepo(selectedRepoId.value)
      void loadLastFetch(selectedRepoId.value)
    }
  } else if (k === 'f') {
    e.preventDefault()
    tab.value = 'history'
    void nextTick(() => {
      const el = document.querySelector('.history-filters input') as HTMLInputElement | null
      el?.focus()
    })
  }
}

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div class="git-page">
    <div class="page-head">
      <h1>Git</h1>
      <span class="page-meta">{{ projects.length }} REPOS</span>
      <span v-if="selected && branches[selected.id]?.length" class="page-meta">
        {{ branches[selected.id].length }} BRANCHES
      </span>
    </div>

    <div v-if="projects.length === 0" class="empty-state">
      <div class="empty-title">还没有项目</div>
      <div class="empty-sub">先在「项目」页添加项目，这里会显示它们的仓库状态</div>
    </div>

    <template v-else>
      <div v-if="gitError" class="git-global-error">{{ gitError }}</div>
      <GitTopBar :projects="projects" />

      <div class="git-tabs">
        <button class="git-tab" :class="{ active: tab === 'changes' }" @click="tab = 'changes'">Changes</button>
        <button class="git-tab" :class="{ active: tab === 'history' }" @click="tab = 'history'">History</button>
      </div>

      <GitChanges v-if="selected && selectedStatus?.isRepo && tab === 'changes'" :project-id="selected.id" />
      <GitHistory v-else-if="selected && selectedStatus?.isRepo" :project-id="selected.id" />
      <div v-else class="empty-state">
        <div class="empty-title">{{ selected?.name || '未选择仓库' }}</div>
        <div class="empty-sub">{{ selectedStatus?.error || '这不是一个 git 仓库' }}</div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { config } from '../store'
import { refreshStatus, refreshStatuses, statuses } from '../gitStore'
import type { RepoStatus } from '../types'
import GitPanel from '../components/GitPanel.vue'

const props = defineProps<{ projectId?: string }>()

const projects = computed(() => config.value?.projects ?? [])
const selectedId = ref(props.projectId ?? '')
const refreshing = ref(false)

const selected = computed(() => projects.value.find((p) => p.id === selectedId.value) ?? null)
const selectedStatus = computed(() => (selected.value ? statuses.value[selected.value.id] : undefined))

onMounted(async () => {
  await refreshAll()
  if (!selected.value) {
    const firstRepo = projects.value.find((p) => statuses.value[p.id]?.isRepo) ?? projects.value[0]
    selectedId.value = firstRepo?.id ?? ''
  }
})

watch(
  () => props.projectId,
  (id) => {
    if (id) selectedId.value = id
  },
)

async function refreshAll() {
  refreshing.value = true
  try {
    await refreshStatuses(projects.value.map((p) => p.id))
  } finally {
    refreshing.value = false
  }
}

function select(id: string) {
  selectedId.value = id
  refreshStatus(id)
}

function dirtyCount(s: RepoStatus | undefined) {
  return s ? s.staged + s.unstaged + s.untracked : 0
}
</script>

<template>
  <div class="git-page">
    <div class="home-head">
      <h1>Git</h1>
      <span class="row">
        <span class="home-count mono">{{ projects.length }} REPOS</span>
        <button class="bordered" :disabled="refreshing" @click="refreshAll">
          {{ refreshing ? '刷新中…' : '刷新' }}
        </button>
      </span>
    </div>

    <div v-if="projects.length === 0" class="empty-state">
      <div class="empty-title">还没有项目</div>
      <div class="empty-sub">先在「项目」页添加项目，这里会显示它们的仓库状态</div>
    </div>

    <div v-else class="git-layout">
      <div class="git-repos">
        <div
          v-for="p in projects"
          :key="p.id"
          class="git-repo"
          :class="{ active: p.id === selectedId }"
          @click="select(p.id)"
        >
          <div class="gr-head">
            <span class="gr-name">{{ p.name || '未命名项目' }}</span>
            <span class="gr-branch mono" :class="{ dim: !statuses[p.id]?.isRepo }">
              {{ statuses[p.id]?.isRepo ? (statuses[p.id]?.detached ? 'detached' : statuses[p.id]?.branch) : '非仓库' }}
            </span>
          </div>
          <div class="gr-meta mono">
            <span v-if="dirtyCount(statuses[p.id])" class="gb-dirty">●{{ dirtyCount(statuses[p.id]) }}</span>
            <span v-if="statuses[p.id]?.ahead" class="gb-ahead">↑{{ statuses[p.id]?.ahead }}</span>
            <span v-if="statuses[p.id]?.behind" class="gb-behind">↓{{ statuses[p.id]?.behind }}</span>
            <span v-if="statuses[p.id]?.error" class="gb-warn" :title="statuses[p.id]?.error ?? undefined">—</span>
          </div>
        </div>
      </div>

      <div class="git-detail-pane">
        <GitPanel v-if="selected && selectedStatus?.isRepo" :project-id="selected.id" />
        <div v-else-if="selected" class="empty-state">
          <div class="empty-title">{{ selected.name || '未命名项目' }}</div>
          <div class="empty-sub">
            {{ selectedStatus?.error || '这不是一个 git 仓库' }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

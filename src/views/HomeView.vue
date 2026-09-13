<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { config, persist } from '../store'
import { newId, type Item, type Project } from '../types'
import { getConfig, launchProject, openDir } from '../api'
import { filterProjects, sortProjects } from '../utils'
import GitBadge from '../components/GitBadge.vue'
import GitPanel from '../components/GitPanel.vue'
import { expandedId, gitError, refreshStatus, refreshStatuses, statuses } from '../gitStore'

const emit = defineEmits<{ edit: [projectId: string]; scan: []; notify: [msg: string, kind?: 'ok' | 'err'] }>()

const projectIds = () => config.value?.projects.map((p) => p.id) ?? []
let focusTimer: number | undefined

onMounted(() => {
  refreshStatuses(projectIds())
  window.addEventListener('focus', onWindowFocus)
})

onUnmounted(() => {
  window.removeEventListener('focus', onWindowFocus)
  clearTimeout(focusTimer)
})

function onWindowFocus() {
  clearTimeout(focusTimer)
  focusTimer = window.setTimeout(() => refreshStatuses(projectIds()), 500)
}

function toggleGit(id: string) {
  const opening = expandedId.value !== id
  expandedId.value = opening ? id : ''
  if (opening) refreshStatus(id)
}

function refreshOne(id: string) {
  refreshStatus(id)
}

const projects = computed(() => config.value?.projects ?? [])
const query = ref('')
const visibleProjects = computed(() => sortProjects(filterProjects(projects.value, query.value)))

async function toggleFavorite(p: Project) {
  const next = !p.favorite
  p.favorite = next
  try {
    await persist()
  } catch (e) {
    p.favorite = !next
    emit('notify', `收藏失败：${e}`, 'err')
  }
}

const confirmDeleteId = ref('')
const launchingId = ref('')
const lastLaunchAt = new Map<string, number>()
let confirmTimer: number | undefined

function removeProject(id: string) {
  if (confirmDeleteId.value !== id) {
    confirmDeleteId.value = id
    clearTimeout(confirmTimer)
    confirmTimer = window.setTimeout(() => (confirmDeleteId.value = ''), 3000)
    return
  }
  clearTimeout(confirmTimer)
  confirmDeleteId.value = ''
  if (!config.value) return
  config.value.projects = config.value.projects.filter((p) => p.id !== id)
  persist()
    .then(() => emit('notify', '项目已删除'))
    .catch((e) => emit('notify', `删除失败：${e}`, 'err'))
}

function itemLabel(i: Item): string {
  const cmd = i.command.trim().split('\n')[0]?.trim().split(/\s+/)[0] || ''
  return (i.name || cmd || '未命名').trim()
}

async function launch(id: string) {
  if (launchingId.value) return
  if (Date.now() - (lastLaunchAt.get(id) ?? 0) < 800) return
  launchingId.value = id
  try {
    await persist()
    await launchProject(id)
    lastLaunchAt.set(id, Date.now())
    emit('notify', '已开始启动…')
    getConfig()
      .then((c) => (config.value = c))
      .catch(() => {})
    refreshStatuses(projectIds())
  } catch (e) {
    emit('notify', `启动失败：${e}`, 'err')
  } finally {
    launchingId.value = ''
  }
}

async function open(path: string) {
  try {
    await openDir(path)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

function createProject() {
  if (!config.value) return
  const p = { id: newId(), name: '新项目', rootDir: '', favorite: false, items: [] }
  config.value.projects.push(p)
  persist().catch((e) => emit('notify', `保存失败：${e}`, 'err'))
  emit('edit', p.id)
}
</script>

<template>
  <div class="home">
    <div class="home-head">
      <h1>启动台</h1>
      <span class="row">
        <input v-model="query" class="inline filter-input mono" placeholder="过滤项目…" spellcheck="false" />
        <span class="home-count mono">{{ projects.length }} PROJECTS</span>
        <button class="bordered" @click="emit('scan')">扫描工作区</button>
        <button class="bordered" @click="createProject">+ 新建项目</button>
      </span>
    </div>

    <div v-if="gitError" class="git-global-error">{{ gitError }}</div>

    <div v-if="projects.length === 0" class="empty-state">
      <div class="empty-title">还没有项目</div>
      <div class="empty-sub">配置一次项目路径和命令，以后一键启动全部终端</div>
      <button class="primary" @click="createProject">+ 新建项目</button>
      <button class="bordered" @click="emit('scan')">扫描工作区</button>
    </div>

    <div v-else-if="visibleProjects.length === 0" class="empty-state">
      <div class="empty-title">无匹配项目</div>
      <div class="empty-sub">换个关键词试试</div>
    </div>

    <div v-else class="list">
      <template v-for="p in visibleProjects" :key="p.id">
        <div
          class="project-row"
          role="button"
          tabindex="0"
          title="点击启动"
          @click="launch(p.id)"
          @keydown.enter.self="launch(p.id)"
        >
          <button class="launch-btn" title="启动" :disabled="launchingId === p.id" @click.stop="launch(p.id)">▶</button>

          <div class="pc-info">
            <div class="pc-head">
              <span class="pc-name">{{ p.name || '未命名项目' }}</span>
              <span class="pc-path mono">{{ p.rootDir || '未设置根目录' }}</span>
              <span class="spacer" />
              <GitBadge
                :status="statuses[p.id]"
                :expanded="expandedId === p.id"
                @toggle="toggleGit(p.id)"
                @refresh="refreshOne(p.id)"
              />
            </div>
            <div class="pc-pipeline">
              <template v-for="(it, i) in p.items" :key="it.id">
                <span v-if="i > 0" class="pl-sep">·</span>
                <span class="pl-cmd" :title="it.command">{{ itemLabel(it) }}</span>
              </template>
              <span v-if="p.items.length === 0" class="pl-empty">无启动项</span>
            </div>
          </div>

          <div class="pc-side">
            <button class="ghost star" :class="{ on: p.favorite }" title="收藏置顶" @click.stop="toggleFavorite(p)">
              {{ p.favorite ? '★' : '☆' }}
            </button>
            <button class="ghost" @click.stop="open(p.rootDir)">打开目录</button>
            <button class="ghost" @click.stop="emit('edit', p.id)">编辑</button>
            <button
              class="danger ghost"
              :class="{ confirming: confirmDeleteId === p.id }"
              @click.stop="removeProject(p.id)"
            >
              {{ confirmDeleteId === p.id ? '确认删除？' : '删除' }}
            </button>
          </div>
        </div>

        <GitPanel v-if="expandedId === p.id" :project-id="p.id" />
      </template>
    </div>
  </div>
</template>

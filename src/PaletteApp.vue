<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getConfig, hidePalette, launchProject, openDir } from './api'
import { filterProjects, sortProjects } from './utils'
import type { AppConfig, Project } from './types'

const cfg = ref<AppConfig | null>(null)
const query = ref('')
const selected = ref(0)
const error = ref('')
const busy = ref(false)
const inputRef = ref<HTMLInputElement>()
const listRef = ref<HTMLElement>()

const projects = computed(() =>
  cfg.value ? sortProjects(filterProjects(cfg.value.projects, query.value)) : [],
)
const hotkey = computed(() => cfg.value?.settings.hotkey ?? '')
const hasProjects = computed(() => (cfg.value?.projects.length ?? 0) > 0)

watch(query, () => {
  selected.value = 0
})

watch(selected, async () => {
  await nextTick()
  listRef.value?.querySelector('.palette-row.active')?.scrollIntoView({ block: 'nearest' })
})

async function reload() {
  try {
    cfg.value = await getConfig()
    error.value = ''
  } catch (e) {
    error.value = `${e}`
  }
  query.value = ''
  selected.value = 0
  await nextTick()
  inputRef.value?.focus()
}

function move(delta: number) {
  if (projects.value.length === 0) return
  selected.value = (selected.value + delta + projects.value.length) % projects.value.length
}

async function launch(p: Project) {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    await launchProject(p.id)
    await hidePalette()
  } catch (e) {
    error.value = `${e}`
  } finally {
    busy.value = false
  }
}

async function openDirOf(p: Project) {
  try {
    await openDir(p.rootDir)
  } catch (e) {
    error.value = `${e}`
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    void hidePalette()
    return
  }
  if (e.isComposing) return
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    move(1)
    return
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault()
    move(-1)
    return
  }
  if (e.key === 'Enter') {
    const p = projects.value[selected.value]
    if (p) void launch(p)
    return
  }
  if (e.ctrlKey && e.key.toLowerCase() === 'o') {
    const p = projects.value[selected.value]
    if (p) void openDirOf(p)
  }
}

let unlisten: (() => void) | undefined

onMounted(async () => {
  await reload()
  unlisten = await listen('palette-shown', () => {
    void reload()
  })
})

onUnmounted(() => unlisten?.())
</script>

<template>
  <div class="palette" @keydown="onKeydown">
    <input
      ref="inputRef"
      v-model="query"
      class="palette-input mono"
      placeholder="搜索项目（名称或路径）…"
      spellcheck="false"
    />
    <div v-if="projects.length > 0" ref="listRef" class="palette-list">
      <div
        v-for="(p, i) in projects"
        :key="p.id"
        class="palette-row"
        :class="{ active: i === selected }"
        @mouseenter="selected = i"
        @click="launch(p)"
      >
        <span class="palette-star" :class="{ on: p.favorite }">{{ p.favorite ? '★' : '☆' }}</span>
        <span class="palette-name">{{ p.name || '未命名项目' }}</span>
        <span class="palette-path mono">{{ p.rootDir }}</span>
        <span class="palette-count mono">{{ p.items.length }} 项</span>
      </div>
    </div>
    <div v-else class="palette-empty">
      {{ hasProjects ? '无匹配项目' : '还没有项目，先在主窗口添加' }}
    </div>
    <div v-if="error" class="palette-error">{{ error }}</div>
    <div class="palette-foot mono">
      <span>↑↓ 选择 · Enter 启动 · Ctrl+O 打开目录 · Esc 关闭</span>
      <span class="spacer" />
      <span>{{ hotkey }}</span>
    </div>
  </div>
</template>

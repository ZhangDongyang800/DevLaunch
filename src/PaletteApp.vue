<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getConfig, getConfigStatus, hidePalette, launchItem, launchProject, openDir } from './api'
import { applyTheme } from './store'
import { filterProjects, itemLabel, sortProjects } from './utils'
import type { AppConfig, ConfigStatus, Item, Project } from './types'

const cfg = ref<AppConfig | null>(null)
const configStatus = ref<ConfigStatus>({ blocked: false, reason: null, path: '' })
const query = ref('')
const selected = ref(0)
const error = ref('')
const busy = ref(false)
const drilled = ref<Project | null>(null)
const inputRef = ref<HTMLInputElement>()
const listRef = ref<HTMLElement>()

const projects = computed(() =>
  cfg.value ? sortProjects(filterProjects(cfg.value.projects, query.value)) : [],
)

const items = computed<Item[]>(() => {
  if (!drilled.value) return []
  const q = query.value.trim().toLowerCase()
  const all = drilled.value.items
  if (!q) return all
  return all.filter(
    (it) =>
      (it.name || '').toLowerCase().includes(q) ||
      it.command.toLowerCase().includes(q) ||
      (it.workDir ?? '').toLowerCase().includes(q),
  )
})

const hotkey = computed(() => cfg.value?.settings.hotkey ?? '')
const hasProjects = computed(() => (cfg.value?.projects.length ?? 0) > 0)

const rows = computed<Item[]>(() => items.value)

const activeOptionId = computed(() => {
  const prefix = drilled.value ? 'palette-item' : 'palette-project'
  const length = drilled.value ? rows.value.length : projects.value.length
  return length ? `${prefix}-${selected.value}` : undefined
})

const listExists = computed(() => (drilled.value ? rows.value.length : projects.value.length) > 0)

watch(query, () => {
  selected.value = 0
})

watch(selected, async () => {
  await nextTick()
  listRef.value?.querySelector('.active')?.scrollIntoView({ block: 'nearest' })
})

async function reload() {
  try {
    const [loaded, status] = await Promise.all([getConfig(), getConfigStatus()])
    cfg.value = loaded
    configStatus.value = status
    applyTheme(cfg.value?.settings.theme)
    error.value = ''
  } catch (e) {
    error.value = `${e}`
  }
  query.value = ''
  selected.value = 0
  drilled.value = null
  await nextTick()
  inputRef.value?.focus()
}

function move(delta: number, len: number) {
  if (len === 0) return
  selected.value = (selected.value + delta + len) % len
}

function drill(p: Project) {
  if (busy.value || p.items.length === 0) return
  drilled.value = p
  query.value = ''
  selected.value = 0
  void nextTick(() => inputRef.value?.focus())
}

function back() {
  if (!drilled.value) return
  const idx = projects.value.findIndex((p) => p.id === drilled.value!.id)
  drilled.value = null
  query.value = ''
  selected.value = idx >= 0 ? idx : 0
  void nextTick(() => inputRef.value?.focus())
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

async function launchOne(it: Item) {
  if (busy.value || !drilled.value) return
  busy.value = true
  error.value = ''
  try {
    await launchItem(drilled.value.id, it.id)
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
  // 输入法组合期间不接管按键：否则中文输入按 Esc 取消候选会直接关掉面板。
  if (e.isComposing) return
  if (e.key === 'Escape') {
    if (drilled.value) {
      back()
      return
    }
    void hidePalette()
    return
  }
  const len = drilled.value ? rows.value.length : projects.value.length
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    move(1, len)
    return
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault()
    move(-1, len)
    return
  }
  if (e.key === 'ArrowLeft') {
    if (drilled.value) {
      e.preventDefault()
      back()
    }
    return
  }
  if (e.key === 'Tab' || e.key === 'ArrowRight') {
    // 面板是纯键盘驱动的：Tab 必须**始终**拦下。此前只在"有项目可钻入"时
    // preventDefault，焦点一旦跑出输入框，↑↓/Enter 就全部失效——面板看起来
    // 就像"卡住了"，用户只能 Esc 关掉重开。
    e.preventDefault()
    if (drilled.value) return
    const p = projects.value[selected.value]
    if (p) drill(p)
    return
  }
  if (e.key === 'Enter') {
    if (drilled.value) {
      const item = rows.value[selected.value]
      if (item) void launchOne(item)
    } else {
      const p = projects.value[selected.value]
      if (p) void launch(p)
    }
    return
  }
  if (e.ctrlKey && e.key.toLowerCase() === 'o' && !drilled.value) {
    const p = projects.value[selected.value]
    if (p) void openDirOf(p)
  }
}

let unlisten: (() => void) | undefined

onMounted(async () => {
  // 挂在 window 上而不是模板根节点：根 div 不可聚焦，`@keydown` 只在输入框
  // 还持有焦点时才收得到——焦点一旦逃逸，整个键盘导航就静默失效。
  window.addEventListener('keydown', onKeydown)
  await reload()
  unlisten = await listen('palette-shown', async () => {
    await reload()
    // 重开时钻入过的项目可能已被删除/改名，别停在过期数据上
    if (drilled.value && !cfg.value?.projects.some((p) => p.id === drilled.value?.id)) {
      drilled.value = null
    }
    await nextTick()
    inputRef.value?.focus()
  })
})

onUnmounted(() => {
  unlisten?.()
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div class="palette">
    <input
      ref="inputRef"
      v-model="query"
      class="palette-input mono"
      :placeholder="drilled ? '搜索启动项（名称 / 命令 / 目录）…' : '搜索项目（名称或路径）…'"
      :aria-label="drilled ? '搜索启动项' : '搜索项目'"
      role="combobox"
      aria-autocomplete="list"
      :aria-controls="listExists ? 'palette-listbox' : undefined"
      :aria-expanded="listExists"
      :aria-activedescendant="activeOptionId"
      spellcheck="false"
    />

    <div v-if="configStatus.blocked" class="config-readonly-banner palette-readonly-banner" role="alert" aria-live="assertive">
      <strong>配置只读</strong>
      <span>{{ configStatus.reason }}</span>
      <span class="mono">{{ configStatus.path }}</span>
      <span>请关闭占用配置文件的程序，确认可读后重启 DevLaunch；本次启动不会保存修改。</span>
    </div>

    <div v-if="drilled" class="palette-crumb">
      <button class="palette-back" type="button" aria-label="返回项目列表" @click="back">←</button>
      <span class="palette-crumb-name">{{ drilled.name || '未命名项目' }}</span>
      <span class="palette-crumb-hint mono">← 返回项目</span>
    </div>

    <template v-if="!drilled">
      <div v-if="projects.length > 0" id="palette-listbox" ref="listRef" class="palette-list" role="listbox" aria-label="项目列表">
        <div
          v-for="(p, i) in projects"
          :id="`palette-project-${i}`"
          :key="p.id"
          class="palette-row"
          :class="{ active: i === selected }"
          role="option"
          :aria-selected="i === selected"
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
    </template>

    <template v-else>
      <div v-if="rows.length > 0" id="palette-listbox" ref="listRef" class="palette-list" role="listbox" aria-label="启动项列表">
        <div
          v-for="(row, i) in rows"
          :id="`palette-item-${i}`"
          :key="row.id"
          class="palette-item"
          :class="{ active: i === selected }"
          role="option"
          :aria-selected="i === selected"
          @mouseenter="selected = i"
          @click="launchOne(row)"
        >
          <span class="pi-name">{{ row.name || '未命名' }}</span>
          <span class="pi-cmd mono">{{ itemLabel(row) }}</span>
          <span class="pi-wd mono">{{ row.workDir || '根目录' }}</span>
        </div>
      </div>
      <div v-else class="palette-empty">
        {{ drilled.items.length === 0 ? '该项目没有启动项' : '无匹配项' }}
      </div>
    </template>

    <div v-if="error" class="palette-error" role="alert" aria-live="assertive" aria-atomic="true">{{ error }}</div>
    <div class="palette-foot mono">
      <span v-if="!drilled">↑↓ 选择 · Enter 启动 · → 展开启动项 · Ctrl+O 打开目录 · Esc 关闭</span>
      <span v-else>↑↓ 选择 · Enter 启动此项 · ← 返回 · Esc 关闭</span>
      <span class="spacer" />
      <span>{{ hotkey }}</span>
    </div>
  </div>
</template>

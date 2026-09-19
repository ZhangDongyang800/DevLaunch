<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getConfig, gitWorktrees, hidePalette, launchItem, launchProject, launchWorktree, openDir } from './api'
import { applyTheme } from './store'
import { filterProjects, sortProjects } from './utils'
import type { AppConfig, Item, Project, WorktreeInfo } from './types'

const cfg = ref<AppConfig | null>(null)
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

// 钻入后内联在启动项上方的环境行；数据在钻入时懒加载一次（面板不做轮询）
const envs = ref<WorktreeInfo[]>([])
const envsError = ref('')
let envSeq = 0

type DrillRow = { kind: 'env'; w: WorktreeInfo } | { kind: 'item'; item: Item }

const visibleEnvs = computed<WorktreeInfo[]>(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return envs.value
  return envs.value.filter((w) => (w.branch ?? '').toLowerCase().includes(q))
})

const rows = computed<DrillRow[]>(() => [
  ...visibleEnvs.value.map((w) => ({ kind: 'env' as const, w })),
  ...items.value.map((item) => ({ kind: 'item' as const, item })),
])

function portOf(branch: string | null): number | null {
  if (!branch) return null
  return drilled.value?.worktree?.leases.find((l) => l.branch === branch)?.port ?? null
}

watch(query, () => {
  selected.value = 0
})

watch(selected, async () => {
  await nextTick()
  listRef.value?.querySelector('.active')?.scrollIntoView({ block: 'nearest' })
})

async function reload() {
  try {
    cfg.value = await getConfig()
    applyTheme(cfg.value?.settings.theme)
    error.value = ''
  } catch (e) {
    error.value = `${e}`
  }
  query.value = ''
  selected.value = 0
  envSeq++
  drilled.value = null
  envs.value = []
  envsError.value = ''
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
  envs.value = []
  envsError.value = ''
  query.value = ''
  selected.value = 0
  void nextTick(() => inputRef.value?.focus())
  if (p.worktree) void loadEnvs(p)
}

async function loadEnvs(p: Project) {
  const mine = ++envSeq
  try {
    const got = await gitWorktrees(p.id)
    if (mine !== envSeq || drilled.value?.id !== p.id) return
    // 只列「有分支、目录还在、不是主工作区」的条目：与启动项并列的行必须是可启动的环境
    envs.value = got.filter((w) => !w.isMain && !w.isPrunable && w.branch)
  } catch (e) {
    if (mine === envSeq) envsError.value = `环境列表读取失败：${e}`
  }
}

function back() {
  if (!drilled.value) return
  const idx = projects.value.findIndex((p) => p.id === drilled.value!.id)
  envSeq++
  drilled.value = null
  envs.value = []
  envsError.value = ''
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

async function launchEnv(w: WorktreeInfo) {
  if (busy.value || !drilled.value || !w.branch) return
  busy.value = true
  error.value = ''
  try {
    await launchWorktree(drilled.value.id, w.branch)
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

function itemLabel(it: Item): string {
  const cmd = it.command.trim().split('\n')[0]?.trim().split(/\s+/)[0] || ''
  return (it.name || cmd || '未命名').trim()
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
    if (drilled.value) return
    const p = projects.value[selected.value]
    if (p) {
      e.preventDefault()
      drill(p)
    }
    return
  }
  if (e.key === 'Enter') {
    if (drilled.value) {
      const row = rows.value[selected.value]
      if (row?.kind === 'env') void launchEnv(row.w)
      else if (row?.kind === 'item') void launchOne(row.item)
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

onUnmounted(() => unlisten?.())
</script>

<template>
  <div class="palette" @keydown="onKeydown">
    <input
      ref="inputRef"
      v-model="query"
      class="palette-input mono"
      :placeholder="drilled ? '搜索环境 / 启动项（名称 / 命令 / 目录）…' : '搜索项目（名称或路径）…'"
      spellcheck="false"
    />

    <div v-if="drilled" class="palette-crumb">
      <button class="palette-back" type="button" @click="back">←</button>
      <span class="palette-crumb-name">{{ drilled.name || '未命名项目' }}</span>
      <span class="palette-crumb-hint mono">← 返回项目</span>
    </div>

    <template v-if="!drilled">
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
    </template>

    <template v-else>
      <div v-if="rows.length > 0" ref="listRef" class="palette-list">
        <div
          v-for="(row, i) in rows"
          :key="row.kind === 'env' ? 'e' + row.w.path : 'i' + row.item.id"
          class="palette-item"
          :class="{ active: i === selected }"
          @mouseenter="selected = i"
          @click="row.kind === 'env' ? launchEnv(row.w) : launchOne(row.item)"
        >
          <template v-if="row.kind === 'env'">
            <span class="pi-name mono">⎇ {{ row.w.branch }}</span>
            <span class="pi-cmd mono">{{ portOf(row.w.branch) != null ? ':' + portOf(row.w.branch) : '' }}</span>
            <span class="pi-wd mono">环境</span>
          </template>
          <template v-else>
            <span class="pi-name">{{ row.item.name || '未命名' }}</span>
            <span class="pi-cmd mono">{{ itemLabel(row.item) }}</span>
            <span class="pi-wd mono">{{ row.item.workDir || '根目录' }}</span>
          </template>
        </div>
      </div>
      <div v-else class="palette-empty">
        {{ drilled.items.length === 0 ? '该项目没有启动项' : '无匹配项' }}
      </div>
    </template>

    <div v-if="error || envsError" class="palette-error">{{ error || envsError }}</div>
    <div class="palette-foot mono">
      <span v-if="!drilled">↑↓ 选择 · Enter 启动 · → 展开启动项 · Ctrl+O 打开目录 · Esc 关闭</span>
      <span v-else>↑↓ 选择 · Enter 启动此项 · ← 返回 · Esc 关闭</span>
      <span class="spacer" />
      <span>{{ hotkey }}</span>
    </div>
  </div>
</template>

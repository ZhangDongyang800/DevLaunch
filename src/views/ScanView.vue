<script setup lang="ts">
import { computed, ref } from 'vue'
import { config, persist } from '../store'
import { newId, type DetectedProject, type Project } from '../types'
import { scanWorkspace } from '../api'

const emit = defineEmits<{ back: []; notify: [msg: string, kind?: 'ok' | 'err'] }>()

const scanPath = ref('')
const scanning = ref(false)
const importing = ref(false)
const results = ref<DetectedProject[]>([])
const checked = ref<boolean[]>([])

const ecosystemLabels: Record<string, string> = { node: 'Node', rust: 'Rust', go: 'Go', python: 'Python' }

const selectedCount = computed(() => checked.value.filter(Boolean).length)
const importableCount = computed(() => results.value.filter((r) => !r.alreadyImported).length)

async function choosePath() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const picked = await open({ directory: true, multiple: false })
    if (typeof picked === 'string') {
      scanPath.value = picked
      results.value = []
      checked.value = []
    }
  } catch (e) {
    emit('notify', `选择目录失败：${e}`, 'err')
  }
}

async function doScan() {
  const root = scanPath.value.trim()
  if (!root || scanning.value) return
  scanning.value = true
  results.value = []
  checked.value = []
  try {
    const found = await scanWorkspace(root)
    results.value = found
    checked.value = found.map((r) => !r.alreadyImported)
    if (found.length === 0) emit('notify', '未在该目录下发现 git 仓库')
  } catch (e) {
    emit('notify', `扫描失败：${e}`, 'err')
  } finally {
    scanning.value = false
  }
}

function toggleAll() {
  const target = selectedCount.value < importableCount.value
  checked.value = results.value.map((r) => !r.alreadyImported && target)
}

async function importSelected() {
  if (!config.value || importing.value) return
  const chosen = results.value.filter((_, i) => checked.value[i])
  if (chosen.length === 0) return
  importing.value = true
  const pushed: Project[] = []
  try {
    for (const r of chosen) {
      const p: Project = {
        id: newId(),
        name: r.name,
        rootDir: r.rootDir,
        favorite: false,
        items: r.suggestions.map((s) => ({
          id: newId(),
          name: s.name,
          workDir: s.workDir ?? '',
          shell: s.shell,
          command: s.command,
        })),
      }
      pushed.push(p)
      config.value.projects.push(p)
    }
    await persist()
    emit('notify', `已导入 ${chosen.length} 个项目`)
    emit('back')
  } catch (e) {
    if (config.value) {
      const ids = new Set(pushed.map((p) => p.id))
      config.value.projects = config.value.projects.filter((p) => !ids.has(p.id))
    }
    emit('notify', `导入失败：${e}`, 'err')
  } finally {
    importing.value = false
  }
}
</script>

<template>
  <div class="scan">
    <div class="editor-head">
      <button class="ghost" @click="emit('back')">← 返回</button>
      <span class="scan-title grow">扫描工作区</span>
    </div>

    <div class="pathbar mono">
      <span class="pb-label">DIR</span>
      <input class="inline" v-model="scanPath" placeholder="D:\Projects" />
      <button class="ghost" @click="choosePath">选择…</button>
      <button class="primary" :disabled="scanning || !scanPath.trim()" @click="doScan">
        {{ scanning ? '扫描中…' : '扫描' }}
      </button>
    </div>

    <p class="hint">扫描目录下两级以内的 git 仓库；已导入的项目置灰。导入后可在编辑器里继续调整启动项。</p>

    <div v-if="results.length > 0" class="scan-list">
      <div class="scan-toolbar">
        <label class="scan-check">
          <input type="checkbox" :checked="selectedCount > 0" @change="toggleAll" />
          全选
        </label>
        <span class="spacer" />
        <span class="mono scan-count">{{ selectedCount }} / {{ importableCount }}</span>
        <button class="primary" :disabled="selectedCount === 0 || importing || scanning" @click="importSelected">
          {{ importing ? '导入中…' : `导入选中（${selectedCount}）` }}
        </button>
      </div>
      <div v-for="(r, ri) in results" :key="r.rootDir" class="scan-row" :class="{ disabled: r.alreadyImported }">
        <input type="checkbox" v-model="checked[ri]" :disabled="r.alreadyImported" />
        <div class="scan-info">
          <div class="scan-name">
            {{ r.name }}
            <span v-if="r.alreadyImported" class="scan-tag">已导入</span>
          </div>
          <div class="scan-path mono">{{ r.rootDir }}</div>
        </div>
        <div class="scan-eco">
          <span v-for="e in r.ecosystems" :key="e" class="scan-badge">{{ ecosystemLabels[e] ?? e }}</span>
          <span v-if="r.ecosystems.length === 0" class="scan-badge dim">未检测到启动项</span>
        </div>
      </div>
    </div>

    <div v-else-if="!scanning && !scanPath.trim()" class="empty-state">
      <div class="empty-title">扫描工作区</div>
      <div class="empty-sub">选择项目父目录（如 D:\Projects），自动发现两级内的 git 仓库并生成建议启动项</div>
    </div>
  </div>
</template>

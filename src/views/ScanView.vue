<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { config, persist } from '../store'
import { isCurrentAsyncResult } from '../utils'
import { newId, type DetectedProject, type Project } from '../types'
import { scanWorkspace } from '../api'

const emit = defineEmits<{ back: []; notify: [msg: string, kind?: 'ok' | 'err'] }>()

const scanPath = ref('')
const scanning = ref(false)
const importing = ref(false)
const results = ref<DetectedProject[]>([])
const checked = ref<boolean[]>([])
const hasScanned = ref(false)
const scanError = ref('')

/**
 * 扫描是异步 IPC，用户完全可能在它返回前切走这一页。此时再写 ref 会把结果
 * 落在已卸载的组件上——下次回到这一页看到的是上一次的残留，而且 `checked`
 * 与 `results` 可能因中途重置而错位。
 */
let alive = true
let scanSeq = 0
onUnmounted(() => {
  alive = false
  scanSeq++
})

watch(scanPath, () => {
  scanSeq++
  scanning.value = false
  results.value = []
  checked.value = []
  hasScanned.value = false
  scanError.value = ''
})

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
      hasScanned.value = false
      scanError.value = ''
    }
  } catch (e) {
    emit('notify', `选择目录失败：${e}`, 'err')
  }
}

async function doScan() {
  const root = scanPath.value.trim()
  if (!root || scanning.value) return
  const seq = ++scanSeq
  scanning.value = true
  results.value = []
  checked.value = []
  hasScanned.value = false
  scanError.value = ''
  try {
    const found = await scanWorkspace(root)
    // 卸载后到达、或被后一次扫描取代的响应一律丢弃
    if (!isCurrentAsyncResult(seq, scanSeq, alive)) return
    results.value = found
    checked.value = found.map((r) => !r.alreadyImported)
    hasScanned.value = true
    if (found.length === 0) emit('notify', '未在该目录下发现 git 仓库')
  } catch (e) {
    if (!isCurrentAsyncResult(seq, scanSeq, alive)) return
    hasScanned.value = true
    scanError.value = `${e}`
    emit('notify', `扫描失败：${e}`, 'err')
  } finally {
    if (isCurrentAsyncResult(seq, scanSeq, alive)) scanning.value = false
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
      <input class="inline" v-model="scanPath" placeholder="D:\Projects" aria-label="扫描目录" :disabled="scanning" />
      <button class="ghost" :disabled="scanning" @click="choosePath">选择…</button>
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
        <input type="checkbox" v-model="checked[ri]" :disabled="r.alreadyImported" :aria-label="`导入 ${r.name}`" />
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

    <div v-else-if="scanning" class="empty-state">
      <div class="empty-title">正在扫描…</div>
      <div class="empty-sub">发现 git 仓库并生成建议启动项</div>
    </div>

    <div v-else-if="scanError" class="empty-state">
      <div class="empty-title">扫描失败</div>
      <div class="empty-sub">{{ scanError }}</div>
      <button class="primary" @click="doScan">重试</button>
    </div>

    <div v-else-if="hasScanned" class="empty-state">
      <div class="empty-title">未发现 Git 仓库</div>
      <div class="empty-sub">该目录两级以内没有可导入的 Git 仓库，请检查目录或扫描范围</div>
      <button class="primary" @click="doScan">重新扫描</button>
    </div>

    <div v-else-if="!scanPath.trim()" class="empty-state">
      <div class="empty-title">扫描工作区</div>
      <div class="empty-sub">选择项目父目录（如 D:\Projects），自动发现两级内的 git 仓库并生成建议启动项</div>
    </div>
  </div>
</template>

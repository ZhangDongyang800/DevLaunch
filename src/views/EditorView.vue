<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { config, persist } from '../store'
import { detectProject, exportProjectFile, getConfig, launchItem, listSubdirs, readProjectTemplate } from '../api'
import { newId, newItem, type DetectResult, type Item } from '../types'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ back: []; notify: [msg: string, kind?: 'ok' | 'err'] }>()

const project = computed(() => config.value!.projects.find((p) => p.id === props.projectId)!)
const savedSnapshot = ref(JSON.stringify(project.value))
const dirty = computed(() => JSON.stringify(project.value) !== savedSnapshot.value)

async function save(): Promise<boolean> {
  try {
    await persist()
    savedSnapshot.value = JSON.stringify(project.value)
    emit('notify', '已保存')
    return true
  } catch (e) {
    emit('notify', `保存失败：${e}`, 'err')
    return false
  }
}

async function goBack() {
  if (dirty.value && !(await save())) return
  emit('back')
}

function addItem() {
  project.value.items.push(newItem())
}

function removeItem(ii: number) {
  project.value.items.splice(ii, 1)
}

function moveItem(ii: number, dir: number) {
  const j = ii + dir
  if (j < 0 || j >= project.value.items.length) return
  ;[project.value.items[ii], project.value.items[j]] = [project.value.items[j], project.value.items[ii]]
}

const subdirs = ref<string[]>([])
const subdirsError = ref('')
const openMenuFor = ref('')
let subdirsTimer: number | undefined

async function refreshSubdirs() {
  const root = project.value?.rootDir
  subdirsError.value = ''
  if (!root) { subdirs.value = []; return }
  try {
    subdirs.value = await listSubdirs(root)
  } catch (e) {
    subdirs.value = []
    subdirsError.value = `${e}`
  }
}
watch(() => project.value?.rootDir, () => {
  clearTimeout(subdirsTimer)
  subdirsTimer = window.setTimeout(() => refreshSubdirs(), 300)
})
// 离开编辑页后这些定时器还会各打一次 IPC / 改一次状态，结果落回已卸载的组件上
onUnmounted(() => {
  clearTimeout(subdirsTimer)
  clearTimeout(confirmExportTimer)
})

function openCombo(itemId: string) {
  openMenuFor.value = itemId
  refreshSubdirs()
}
function closeCombo() { openMenuFor.value = '' }
function pickWorkDir(it: Item, dir: string) {
  it.workDir = dir
  closeCombo()
}

const runningItemId = ref('')

async function tryRunItem(it: Item) {
  if (runningItemId.value) return
  runningItemId.value = it.id
  try {
    await persist()
    savedSnapshot.value = JSON.stringify(project.value)
    await launchItem(project.value.id, it.id)
    emit('notify', `已启动「${it.name || '启动项'}」`)
    getConfig()
      .then((c) => {
        if (dirty.value) return
        config.value = c
        savedSnapshot.value = JSON.stringify(project.value)
      })
      .catch(() => {})
  } catch (e) {
    emit('notify', `${e}`, 'err')
  } finally {
    runningItemId.value = ''
  }
}

function ensureItemIds(items: Item[]) {
  const seen = new Set<string>()
  for (const it of items) {
    if (!it.id || seen.has(it.id)) it.id = newId()
    seen.add(it.id)
  }
}

async function browseRoot() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ directory: true, multiple: false })
  if (typeof picked !== 'string') return
  project.value.rootDir = picked
  await tryAutoImport()
  if (project.value.items.length === 0) await runDetect(false)
}

function projectFilePath() {
  const root = project.value.rootDir.replace(/[\\/]+$/, '')
  return root ? `${root}\\devlaunch.json` : ''
}

async function tryAutoImport() {
  const path = projectFilePath()
  if (!path || project.value.items.length > 0) return
  try {
    const tpl = await readProjectTemplate(path)
    if (tpl.items.length > 0) {
      ensureItemIds(tpl.items)
      project.value.name = tpl.name || project.value.name
      project.value.items = tpl.items
      emit('notify', '已从项目根目录 devlaunch.json 导入启动项')
    }
  } catch {
    // 没有项目文件时静默
  }
}

const detectResult = ref<DetectResult | null>(null)
const detectChecked = ref<boolean[]>([])
const detecting = ref(false)
watch(() => project.value?.rootDir, () => {
  detectResult.value = null
  detectChecked.value = []
})

const ecosystemLabels: Record<string, string> = { node: 'Node', rust: 'Rust', go: 'Go', python: 'Python' }

const detectTitle = computed(() => {
  if (!detectResult.value) return ''
  const names = detectResult.value.ecosystems.map((e) => ecosystemLabels[e] ?? e)
  return `检测到 ${names.join(' + ')} 项目，建议 ${detectResult.value.suggestions.length} 个启动项`
})

async function runDetect(notifyError: boolean) {
  const root = project.value.rootDir.trim()
  if (!root) {
    if (notifyError) emit('notify', '请先设置项目根目录', 'err')
    return
  }
  if (detecting.value) return
  detecting.value = true
  try {
    const res = await detectProject(root)
    if (res.suggestions.length === 0) {
      detectResult.value = null
      if (notifyError) emit('notify', '未检测到可生成的启动项')
    } else {
      detectResult.value = res
      detectChecked.value = res.suggestions.map(() => true)
    }
  } catch (e) {
    detectResult.value = null
    if (notifyError) emit('notify', `检测失败：${e}`, 'err')
  } finally {
    detecting.value = false
  }
}

function applyDetection() {
  if (!detectResult.value) return
  const chosen = detectResult.value.suggestions.filter((_, i) => detectChecked.value[i])
  for (const s of chosen) {
    project.value.items.push({
      id: newId(),
      name: s.name,
      workDir: s.workDir ?? '',
      shell: s.shell,
      command: s.command,
    })
  }
  detectResult.value = null
  emit('notify', `已添加 ${chosen.length} 个建议启动项`)
}

function ignoreDetection() {
  detectResult.value = null
}

const confirmExport = ref(false)
let confirmExportTimer: number | undefined

async function doExportToRoot() {
  const path = projectFilePath()
  if (!path) { emit('notify', '请先设置项目根目录', 'err'); return }
  if (!confirmExport.value) {
    confirmExport.value = true
    clearTimeout(confirmExportTimer)
    confirmExportTimer = window.setTimeout(() => (confirmExport.value = false), 3000)
    return
  }
  clearTimeout(confirmExportTimer)
  confirmExport.value = false
  try {
    await persist()
    savedSnapshot.value = JSON.stringify(project.value)
  } catch (e) {
    emit('notify', `保存失败：${e}`, 'err')
    return
  }
  try {
    // 用后端返回的实际路径提示，前端拼的 projectFilePath() 只是预判
    const written = await exportProjectFile(project.value.id)
    emit('notify', `已导出到 ${written}`)
  } catch (e) {
    emit('notify', `导出失败：${e}`, 'err')
  }
}

async function doImport() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] })
  if (typeof picked !== 'string') return
  try {
    const tpl = await readProjectTemplate(picked)
    ensureItemIds(tpl.items)
    project.value.name = tpl.name || project.value.name
    project.value.items = tpl.items
    await persist()
    savedSnapshot.value = JSON.stringify(project.value)
    emit('notify', '项目配置已导入')
  } catch (e) {
    emit('notify', `导入失败：${e}`, 'err')
  }
}
</script>

<template>
  <div class="editor" v-if="project">
    <div class="editor-head">
      <button class="ghost" @click="goBack">← 返回</button>
      <span v-if="dirty" class="dirty-dot" title="有未保存的更改" />
      <input v-model="project.name" class="editor-title grow" placeholder="项目名称" />
      <button class="ghost" title="从项目配置文件导入（覆盖启动项，保留根目录）" @click="doImport">导入</button>
      <button
        class="ghost"
        :class="{ confirming: confirmExport }"
        title="导出为项目根目录下的 devlaunch.json"
        @click="doExportToRoot"
      >
        {{ confirmExport ? '确认覆盖？' : '导出到项目根' }}
      </button>
      <button class="primary" @click="save">保存</button>
    </div>

    <div class="pathbar mono">
      <span class="pb-label">ROOT</span>
      <input class="inline" v-model="project.rootDir" placeholder="D:\Projects\my-app" />
      <button class="ghost" @click="browseRoot">选择…</button>
      <button class="ghost" :disabled="detecting" @click="runDetect(true)">
        {{ detecting ? '检测中…' : '检测项目' }}
      </button>
    </div>

    <div v-if="detectResult" class="detect-card">
      <div class="dc-head">
        <span class="dc-title">{{ detectTitle }}</span>
        <span class="spacer" />
        <button class="ghost" @click="ignoreDetection">忽略</button>
      </div>
      <label v-for="(s, si) in detectResult.suggestions" :key="si" class="dc-row">
        <input type="checkbox" v-model="detectChecked[si]" />
        <span class="dc-name">{{ s.name }}</span>
        <span class="dc-wd mono">{{ s.workDir || '根目录' }}</span>
        <span class="dc-cmd mono">{{ s.command }}</span>
        <span class="dc-badge">{{ ecosystemLabels[s.ecosystem] ?? s.ecosystem }}</span>
      </label>
      <button class="primary" @click="applyDetection">添加选中项</button>
    </div>

    <div v-for="(it, ii) in project.items" :key="it.id" class="group">
      <div class="group-head">
        <span class="group-index">{{ String(ii + 1).padStart(2, '0') }}</span>
        <input v-model="it.name" class="group-name grow" placeholder="启动项名称（如 后端）" />
        <select v-model="it.shell" title="高级：命令方言（默认 CMD）">
          <option value="cmd">CMD</option>
          <option value="powershell">PowerShell</option>
          <option value="bash">Git Bash</option>
        </select>
        <button class="accent" :disabled="runningItemId !== ''" @click="tryRunItem(it)">▶ 运行此项</button>
        <button class="danger ghost" @click="removeItem(ii)">✕</button>
      </div>

      <div class="steps">
        <div class="step-item">
          <div class="rail"><span class="step-dot">⌘</span></div>
          <div class="step-body">
            <div class="s-main">
              <textarea
                v-model="it.command"
                class="cmd-input"
                rows="3"
                placeholder="按平时手动敲的顺序写，一行一条（如 conda activate xingtu 换行 python -m uvicorn main:app --port 8081）"
                spellcheck="false"
              />
            </div>
            <div class="s-meta">
              <div class="combo">
                <input
                  class="inline"
                  v-model="it.workDir"
                  placeholder="子目录（留空=根目录）"
                  @focus="openCombo(it.id)"
                  @blur="closeCombo"
                  @keydown.esc="closeCombo"
                />
                <div v-if="openMenuFor === it.id" class="combo-menu">
                  <div class="combo-item" :class="{ active: !it.workDir }" @mousedown.prevent="pickWorkDir(it, '')">
                    （根目录）
                  </div>
                  <div
                    v-for="d in subdirs"
                    :key="d"
                    class="combo-item mono"
                    :class="{ active: it.workDir === d }"
                    @mousedown.prevent="pickWorkDir(it, d)"
                  >
                    {{ d }}
                  </div>
                  <div v-if="subdirsError" class="combo-empty">{{ subdirsError }}</div>
                  <div v-else-if="subdirs.length === 0" class="combo-empty">根目录下没有子目录，可手动输入相对路径</div>
                </div>
              </div>
              <span class="spacer" />
              <button class="ghost" :disabled="ii === 0" title="上移" @click="moveItem(ii, -1)">↑</button>
              <button class="ghost" :disabled="ii === project.items.length - 1" title="下移" @click="moveItem(ii, 1)">↓</button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <button class="ghost" style="width: 100%; border: 1px dashed var(--border-strong)" @click="addItem">
      + 添加启动项
    </button>

    <p class="hint">
      每个启动项 = 一个真实终端窗格：多行命令在同一个 shell 会话里按顺序执行（cd → 激活环境 → 启动服务）。一个项目一键启动 = 一个 Windows Terminal 窗口，每个启动项一个窗格，默认并行启动。需要等待时在命令里自己写等待（CMD：timeout /t 5 /nobreak &gt;nul；PowerShell：Start-Sleep -Seconds 5）。
    </p>
  </div>
</template>

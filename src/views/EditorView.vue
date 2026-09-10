<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { config, persist } from '../store'
import { exportProject, launchGroup, listSubdirs, readProjectTemplate, runStep } from '../api'
import { newGroup, newStep, type Group, type ReadyCondition, type Step } from '../types'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ back: []; notify: [msg: string, kind?: 'ok' | 'err'] }>()

const project = computed(() => config.value!.projects.find((p) => p.id === props.projectId)!)

const savedSnapshot = ref(JSON.stringify(project.value))
const dirty = computed(() => JSON.stringify(project.value) !== savedSnapshot.value)

const expanded = ref(new Set<string>())

function toggleAdvanced(id: string) {
  const next = new Set(expanded.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expanded.value = next
}

async function save() {
  try {
    await persist()
    savedSnapshot.value = JSON.stringify(project.value)
    emit('notify', '已保存')
  } catch (e) {
    emit('notify', `保存失败：${e}`, 'err')
  }
}

function addGroup() {
  project.value.groups.push(newGroup(project.value.groups.length))
}

function removeGroup(gi: number) {
  project.value.groups.splice(gi, 1)
}

function addStep(g: Group) {
  g.steps.push(newStep())
}

function removeStep(g: Group, si: number) {
  g.steps.splice(si, 1)
}

function moveStep(g: Group, si: number, dir: number) {
  const j = si + dir
  if (j < 0 || j >= g.steps.length) return
  ;[g.steps[si], g.steps[j]] = [g.steps[j], g.steps[si]]
}

function onCondTypeChange(step: Step, t: string) {
  const timeout = config.value!.settings.readyTimeoutSec
  if (t === 'immediate') step.readyCondition = { type: 'immediate' }
  else if (t === 'delay') step.readyCondition = { type: 'delay', seconds: 3 }
  else if (t === 'port') step.readyCondition = { type: 'port', port: 8000, host: '127.0.0.1', timeoutSec: timeout }
  else step.readyCondition = { type: 'process', processName: '', timeoutSec: timeout }
}

function gateText(c: ReadyCondition): string {
  if (c.type === 'delay') return `${c.seconds}s`
  if (c.type === 'port') return `PORT ${c.port} READY`
  if (c.type === 'process') return `PROC ${c.processName || '?'}`
  return '立即'
}

function cmdRows(s: Step): number {
  return Math.min(4, Math.max(1, s.command.split('\n').length))
}

const subdirs = ref<string[]>([])
const openMenuFor = ref('')

async function refreshSubdirs() {
  const root = project.value?.rootDir
  if (!root) {
    subdirs.value = []
    return
  }
  try {
    subdirs.value = await listSubdirs(root)
  } catch {
    subdirs.value = []
  }
}

function openCombo(stepId: string) {
  openMenuFor.value = stepId
  refreshSubdirs()
}

function closeCombo() {
  openMenuFor.value = ''
}

function pickWorkDir(s: Step, dir: string) {
  s.workDir = dir
  closeCombo()
}

watch(
  () => project.value?.rootDir,
  () => refreshSubdirs()
)

async function tryRunStep(g: Group, s: Step) {
  try {
    await runStep(project.value.id, g.id, s.id)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function tryRunGroup(g: Group) {
  try {
    await launchGroup(project.value.id, g.id)
    emit('notify', '分组已开始启动…')
  } catch (e) {
    emit('notify', `启动失败：${e}`, 'err')
  }
}

async function browseRoot() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ directory: true, multiple: false })
  if (typeof picked === 'string') project.value.rootDir = picked
}

async function doExport() {
  const { save } = await import('@tauri-apps/plugin-dialog')
  const base = (project.value.name || 'project').trim() || 'project'
  const path = await save({
    defaultPath: `${base}-devlaunch.json`,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (!path) return
  try {
    await exportProject(project.value.id, path)
    emit('notify', '项目配置已导出（不含根目录路径）')
  } catch (e) {
    emit('notify', `导出失败：${e}`, 'err')
  }
}

const importArmed = ref(false)
let importTimer: number | undefined

async function doImport() {
  if (!importArmed.value && project.value.groups.some((g) => g.steps.length > 0)) {
    importArmed.value = true
    clearTimeout(importTimer)
    importTimer = window.setTimeout(() => (importArmed.value = false), 3000)
    return
  }
  importArmed.value = false
  clearTimeout(importTimer)
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    multiple: false,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (typeof picked !== 'string') return
  try {
    const tpl = await readProjectTemplate(picked)
    project.value.name = tpl.name || project.value.name
    project.value.groups = tpl.groups
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
      <button class="ghost" @click="emit('back')">← 返回</button>
      <span v-if="dirty" class="dirty-dot" title="有未保存的更改" />
      <input v-model="project.name" class="editor-title grow" placeholder="项目名称" />
      <button
        class="ghost"
        :class="{ confirming: importArmed }"
        :title="importArmed ? '再次点击确认覆盖当前步骤' : '从项目配置文件导入（覆盖当前步骤，保留根目录）'"
        @click="doImport"
      >
        {{ importArmed ? '确认覆盖？' : '导入' }}
      </button>
      <button class="ghost" title="导出项目配置文件（不含根目录，可分享给其他机器）" @click="doExport">导出</button>
      <button class="primary" @click="save">保存</button>
    </div>

    <div class="pathbar mono">
      <span class="pb-label">ROOT</span>
      <input class="inline" v-model="project.rootDir" placeholder="D:\Projects\my-app" />
      <button class="ghost" @click="browseRoot">选择…</button>
    </div>

    <div v-for="(g, gi) in project.groups" :key="g.id" class="group">
      <div class="group-head">
        <span class="group-index">{{ String(gi + 1).padStart(2, '0') }}</span>
        <input v-model="g.name" class="group-name grow" />
        <select v-model="g.terminal" title="本组使用的终端（一组 = 一个终端窗口）">
          <option value="cmd">CMD</option>
          <option value="powershell">PowerShell</option>
          <option value="windowsterminal">Windows Terminal</option>
        </select>
        <button class="accent" @click="tryRunGroup(g)">▶ 运行本组</button>
        <button class="danger ghost" @click="removeGroup(gi)">✕</button>
      </div>

      <ol class="steps">
        <li v-for="(s, si) in g.steps" :key="s.id" class="step-item">
          <div class="rail">
            <span class="step-dot">{{ si + 1 }}</span>
            <span v-if="si < g.steps.length - 1" class="rail-line" />
          </div>

          <div class="step-body">
            <div class="s-main">
              <textarea
                v-model="s.command"
                class="cmd-input"
                :rows="cmdRows(s)"
                placeholder="命令，如 npm run dev（支持多行：同一终端窗口内顺序执行，如先 conda activate 再启动）"
                spellcheck="false"
              />
              <button class="run-step" title="单步运行" @click="tryRunStep(g, s)">▶ RUN</button>
            </div>

            <div class="s-meta">
              <input class="inline" v-model="s.name" placeholder="名称" title="名称（可选）" />
              <div class="combo">
                <input
                  class="inline"
                  v-model="s.workDir"
                  placeholder="子目录（留空=根目录）"
                  title="工作目录：从根目录下的子目录中选择，或手动输入相对路径"
                  @focus="openCombo(s.id)"
                  @blur="closeCombo"
                  @keydown.esc="closeCombo"
                />
                <div v-if="openMenuFor === s.id" class="combo-menu">
                  <div
                    class="combo-item"
                    :class="{ active: !s.workDir }"
                    @mousedown.prevent="pickWorkDir(s, '')"
                  >
                    （根目录）
                  </div>
                  <div
                    v-for="d in subdirs"
                    :key="d"
                    class="combo-item mono"
                    :class="{ active: s.workDir === d }"
                    @mousedown.prevent="pickWorkDir(s, d)"
                  >
                    {{ d }}
                  </div>
                  <div v-if="subdirs.length === 0" class="combo-empty">根目录下没有子目录，可手动输入相对路径</div>
                </div>
              </div>
              <select
                :value="s.readyCondition.type"
                title="此步完成后的等待条件"
                @change="onCondTypeChange(s, ($event.target as HTMLSelectElement).value)"
              >
                <option value="immediate">完成后：立即</option>
                <option value="delay">完成后：延迟</option>
                <option value="port">完成后：等端口</option>
                <option value="process">完成后：等进程</option>
              </select>
              <span class="spacer" />
              <button
                v-if="s.readyCondition.type !== 'immediate'"
                class="adv-toggle mono"
                @click="toggleAdvanced(s.id)"
              >
                {{ expanded.has(s.id) ? '▾ 参数' : '▸ 参数' }}
              </button>
              <button class="ghost" :disabled="si === 0" title="上移" @click="moveStep(g, si, -1)">↑</button>
              <button class="ghost" :disabled="si === g.steps.length - 1" title="下移" @click="moveStep(g, si, 1)">↓</button>
              <button class="danger ghost" title="删除步骤" @click="removeStep(g, si)">✕</button>
            </div>

            <div
              v-show="s.readyCondition.type !== 'immediate' && expanded.has(s.id)"
              class="cond-fields"
            >
              <template v-if="s.readyCondition.type === 'delay'">
                <label>等待秒数 <input type="number" class="mono" v-model.number="(s.readyCondition as any).seconds" min="0" /></label>
              </template>
              <template v-else-if="s.readyCondition.type === 'port'">
                <label>主机 <input class="mono" v-model="(s.readyCondition as any).host" /></label>
                <label>端口 <input type="number" class="mono" v-model.number="(s.readyCondition as any).port" min="1" max="65535" /></label>
                <label>超时秒 <input type="number" class="mono" v-model.number="(s.readyCondition as any).timeoutSec" min="0" /></label>
              </template>
              <template v-else-if="s.readyCondition.type === 'process'">
                <label>进程名（如 python.exe，不要填终端自身）<input class="mono" v-model="(s.readyCondition as any).processName" /></label>
                <label>超时秒 <input type="number" class="mono" v-model.number="(s.readyCondition as any).timeoutSec" min="0" /></label>
              </template>
            </div>

            <div v-if="si < g.steps.length - 1" class="gate-note">
              <span>└ 完成后 →</span>
              <span class="g-signal">{{ gateText(s.readyCondition) }}</span>
            </div>
          </div>
        </li>
      </ol>

      <div class="group-foot">
        <button class="ghost" @click="addStep(g)">+ 添加步骤</button>
      </div>
    </div>

    <button class="ghost" style="width: 100%; border: 1px dashed var(--border-strong)" @click="addGroup">
      + 添加分组
    </button>

    <p class="hint">
      一个分组 = 一个终端窗口：组内步骤在同一终端里顺序执行（环境状态如 conda activate 对后续步骤生效）。上一步按"完成条件"在终端内等待后再执行下一步，最后一个阻塞命令（如启动服务）常驻该窗口；组与组之间在首页逐组手动运行。就绪条件等待超时会在终端窗口内提示并停住后续步骤。
    </p>
  </div>
</template>

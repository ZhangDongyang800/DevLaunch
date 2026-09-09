<script setup lang="ts">
import { computed, ref } from 'vue'
import { config, persist } from '../store'
import { launchGroup, runStep } from '../api'
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
</script>

<template>
  <div class="editor" v-if="project">
    <div class="editor-head">
      <button class="ghost" @click="emit('back')">← 返回</button>
      <span v-if="dirty" class="dirty-dot" title="有未保存的更改" />
      <input v-model="project.name" class="editor-title grow" placeholder="项目名称" />
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
              <input v-model="s.command" class="cmd-input" placeholder="命令，如 npm run dev" />
              <button class="run-step" title="单步运行" @click="tryRunStep(g, s)">▶ RUN</button>
            </div>

            <div class="s-meta">
              <input class="inline" v-model="s.name" placeholder="名称" title="名称（可选）" />
              <input class="inline" v-model="s.workDir" placeholder="子目录（留空=根目录）" title="工作目录" />
              <select v-model="s.terminal" title="终端">
                <option value="cmd">CMD</option>
                <option value="powershell">PowerShell</option>
                <option value="windowsterminal">Windows Terminal</option>
              </select>
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
      组内按顺序启动：上一步按"完成条件"等待后再启动下一步；组与组之间在首页逐组手动运行。就绪条件类型切换后请重新填写参数。
    </p>
  </div>
</template>

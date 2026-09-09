<script setup lang="ts">
import { computed } from 'vue'
import { config, persist } from '../store'
import { launchGroup, runStep } from '../api'
import { newGroup, newStep, type Group, type Step } from '../types'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ back: []; notify: [msg: string] }>()

const project = computed(() => config.value!.projects.find((p) => p.id === props.projectId)!)

async function save() {
  try {
    await persist()
    emit('notify', '已保存')
  } catch (e) {
    emit('notify', `保存失败：${e}`)
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

async function tryRunStep(g: Group, s: Step) {
  try {
    await runStep(project.value.id, g.id, s.id)
  } catch (e) {
    emit('notify', `${e}`)
  }
}

async function tryRunGroup(g: Group) {
  try {
    await launchGroup(project.value.id, g.id)
    emit('notify', '分组已开始启动…')
  } catch (e) {
    emit('notify', `启动失败：${e}`)
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
    <div class="row">
      <button @click="emit('back')">← 返回</button>
      <h1 class="grow">{{ project.name || '未命名项目' }}</h1>
      <button class="primary" @click="save">保存</button>
    </div>

    <div class="group-card">
      <label>项目名称 <input v-model="project.name" /></label>
      <label style="margin-top: 8px">根目录
        <span class="row">
          <input class="grow" v-model="project.rootDir" />
          <button @click="browseRoot">选择…</button>
        </span>
      </label>
    </div>

    <div v-for="(g, gi) in project.groups" :key="g.id" class="group-card">
      <div class="row">
        <input v-model="g.name" class="grow" />
        <button @click="tryRunGroup(g)">运行本组</button>
        <button class="danger" @click="removeGroup(gi)">删除组</button>
      </div>

      <div v-for="(s, si) in g.steps" :key="s.id" class="step-row" style="margin-top: 10px">
        <input v-model="s.name" placeholder="名称" />
        <input v-model="s.command" placeholder="命令，如 npm run dev" />
        <input v-model="s.workDir" placeholder="子目录（留空=根目录）" />
        <select v-model="s.terminal">
          <option value="cmd">CMD</option>
          <option value="powershell">PowerShell</option>
          <option value="windowsterminal">Windows Terminal</option>
        </select>
        <select :value="s.readyCondition.type" @change="onCondTypeChange(s, ($event.target as HTMLSelectElement).value)">
          <option value="immediate">就绪：立即</option>
          <option value="delay">就绪：延迟</option>
          <option value="port">就绪：端口</option>
          <option value="process">就绪：进程</option>
        </select>
        <span class="row">
          <button @click="moveStep(g, si, -1)" :disabled="si === 0">↑</button>
          <button @click="moveStep(g, si, 1)" :disabled="si === g.steps.length - 1">↓</button>
          <button @click="tryRunStep(g, s)">运行</button>
          <button class="danger" @click="removeStep(g, si)">✕</button>
        </span>

        <div class="cond-fields" v-if="s.readyCondition.type === 'delay'">
          <label>等待秒数 <input type="number" v-model.number="(s.readyCondition as any).seconds" min="0" /></label>
        </div>
        <div class="cond-fields" v-else-if="s.readyCondition.type === 'port'">
          <label>主机 <input v-model="(s.readyCondition as any).host" /></label>
          <label>端口 <input type="number" v-model.number="(s.readyCondition as any).port" min="1" max="65535" /></label>
          <label>超时秒 <input type="number" v-model.number="(s.readyCondition as any).timeoutSec" min="0" /></label>
        </div>
        <div class="cond-fields" v-else-if="s.readyCondition.type === 'process'">
          <label>进程名（如 python.exe，不要填终端自身）<input v-model="(s.readyCondition as any).processName" /></label>
          <label>超时秒 <input type="number" v-model.number="(s.readyCondition as any).timeoutSec" min="0" /></label>
        </div>
      </div>

      <div style="margin-top: 10px">
        <button @click="addStep(g)">+ 添加步骤</button>
      </div>
    </div>

    <button class="add" @click="addGroup">+ 添加分组</button>
    <p class="hint">组内按顺序启动：上一步按"就绪条件"等待后再启动下一步；组与组之间在首页逐组手动运行。就绪条件类型切换后请重新填写参数。</p>
  </div>
</template>

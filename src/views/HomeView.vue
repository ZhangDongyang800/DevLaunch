<script setup lang="ts">
import { computed, ref } from 'vue'
import { config, persist } from '../store'
import { newId, type Project, type ReadyCondition, type Step } from '../types'
import { launchProject, openDir } from '../api'

const emit = defineEmits<{ edit: [projectId: string]; notify: [msg: string, kind?: 'ok' | 'err'] }>()

const projects = computed(() => config.value?.projects ?? [])

const confirmDeleteId = ref('')
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

function projectSteps(p: Project): Step[] {
  return p.groups.flatMap((g) => g.steps)
}

function stepLabel(s: Step): string {
  const cmd = s.command.trim().split(/\s+/)[0] || ''
  return (s.name || cmd || '未命名').trim()
}

function gateText(c: ReadyCondition): string | null {
  if (c.type === 'delay') return `${c.seconds}s`
  if (c.type === 'port') return `PORT ${c.port} READY`
  if (c.type === 'process') return `PROC ${c.processName || '?'}`
  return null
}

async function launch(id: string) {
  try {
    await launchProject(id)
    emit('notify', '已开始启动…')
  } catch (e) {
    emit('notify', `启动失败：${e}`, 'err')
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
  config.value.projects.push({ id: newId(), name: '新项目', rootDir: '', groups: [] })
  emit('edit', config.value.projects[config.value.projects.length - 1].id)
}
</script>

<template>
  <div class="home">
    <div class="home-head">
      <h1>启动台</h1>
      <span class="row">
        <span class="home-count mono">{{ projects.length }} PROJECTS</span>
        <button class="bordered" @click="createProject">+ 新建项目</button>
      </span>
    </div>

    <div v-if="projects.length === 0" class="empty-state">
      <div class="empty-title">还没有项目</div>
      <div class="empty-sub">配置一次项目路径和命令，以后一键启动全部终端</div>
      <button class="primary" @click="createProject">+ 新建项目</button>
    </div>

    <div v-else class="list">
      <div
        v-for="p in projects"
        :key="p.id"
        class="project-row"
        role="button"
        tabindex="0"
        title="点击启动"
        @click="launch(p.id)"
        @keydown.enter="launch(p.id)"
      >
        <button class="launch-btn" title="启动" @click.stop="launch(p.id)">▶</button>

        <div class="pc-info">
          <div class="pc-head">
            <span class="pc-name">{{ p.name || '未命名项目' }}</span>
            <span class="pc-path mono">{{ p.rootDir || '未设置根目录' }}</span>
          </div>
          <div class="pc-pipeline">
            <template v-for="(s, i) in projectSteps(p)" :key="s.id">
              <span v-if="i > 0" class="pl-sep">→</span>
              <span class="pl-cmd" :title="s.command">{{ stepLabel(s) }}</span>
              <template v-if="i < projectSteps(p).length - 1 && gateText(s.readyCondition)">
                <span class="pl-sep">→</span>
                <span class="pl-gate">{{ gateText(s.readyCondition) }}</span>
              </template>
            </template>
            <span v-if="projectSteps(p).length === 0" class="pl-empty">无步骤</span>
          </div>
        </div>

        <div class="pc-side">
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
    </div>
  </div>
</template>

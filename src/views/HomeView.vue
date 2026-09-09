<script setup lang="ts">
import { computed } from 'vue'
import { config } from '../store'
import { newId } from '../types'
import { launchProject, openDir } from '../api'

const emit = defineEmits<{ edit: [projectId: string]; notify: [msg: string] }>()

const projects = computed(() => config.value?.projects ?? [])

async function launch(id: string) {
  try {
    await launchProject(id)
    emit('notify', '已开始启动…')
  } catch (e) {
    emit('notify', `启动失败：${e}`)
  }
}

async function open(path: string) {
  try {
    await openDir(path)
  } catch (e) {
    emit('notify', `${e}`)
  }
}
</script>

<template>
  <div class="home">
    <h1>项目</h1>
    <p v-if="projects.length === 0" class="empty">还没有项目。点击右上角「新建项目」开始配置。</p>
    <div v-for="p in projects" :key="p.id" class="project-row">
      <span class="name">{{ p.name }}</span>
      <span class="actions">
        <button class="primary" @click="launch(p.id)">启动</button>
        <button @click="open(p.rootDir)">打开目录</button>
        <button @click="emit('edit', p.id)">编辑</button>
      </span>
    </div>
    <button
      v-if="config"
      class="primary add"
      @click="config.projects.push({ id: newId(), name: '新项目', rootDir: '', groups: [] }); emit('edit', config.projects[config.projects.length - 1].id)"
    >
      新建项目
    </button>
  </div>
</template>

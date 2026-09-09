<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { load, config } from './store'
import type { View } from './types'
import HomeView from './views/HomeView.vue'
import EditorView from './views/EditorView.vue'
import SettingsView from './views/SettingsView.vue'

const view = ref<View>({ name: 'home' })
const toast = ref('')
let toastTimer: number | undefined

function showToast(msg: string) {
  toast.value = msg
  clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => (toast.value = ''), 5000)
}

onMounted(async () => {
  await load()
  await listen<string | null>('launch-result', (e) => {
    if (e.payload) showToast(`启动中断：${e.payload}`)
    else showToast('启动完成')
  })
})
</script>

<template>
  <div class="app" v-if="config">
    <nav>
      <button :class="{ active: view.name === 'home' }" @click="view = { name: 'home' }">项目</button>
      <button :class="{ active: view.name === 'settings' }" @click="view = { name: 'settings' }">设置</button>
    </nav>
    <main>
      <HomeView v-if="view.name === 'home'" @edit="view = { name: 'editor', projectId: $event }" @notify="showToast" />
      <EditorView v-else-if="view.name === 'editor'" :project-id="view.projectId" @back="view = { name: 'home' }" @notify="showToast" />
      <SettingsView v-else @notify="showToast" />
    </main>
    <div v-if="toast" class="toast">{{ toast }}</div>
  </div>
</template>

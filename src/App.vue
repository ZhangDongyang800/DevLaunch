<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { load, config } from './store'
import type { View, ToastKind } from './types'
import TitleBar from './components/TitleBar.vue'
import HomeView from './views/HomeView.vue'
import ScanView from './views/ScanView.vue'
import EditorView from './views/EditorView.vue'
import SettingsView from './views/SettingsView.vue'

const view = ref<View>({ name: 'home' })
const toast = ref('')
const toastKind = ref<ToastKind>('ok')
const loadError = ref('')
let toastTimer: number | undefined
let unlisten: (() => void) | undefined

function showToast(msg: string, kind: ToastKind = 'ok') {
  toast.value = msg
  toastKind.value = kind
  clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => (toast.value = ''), 5000)
}

async function init() {
  try {
    await load()
    loadError.value = ''
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  }
}

onMounted(async () => {
  await init()
  unlisten = await listen<string>('launch-error', (e) => showToast(e.payload, 'err'))
})

onUnmounted(() => unlisten?.())
</script>

<template>
  <div class="app" v-if="config">
    <TitleBar>
      <button class="tb-nav-item" :class="{ active: view.name === 'home' }" @click="view = { name: 'home' }">
        项目
      </button>
      <button class="tb-nav-item" :class="{ active: view.name === 'settings' }" @click="view = { name: 'settings' }">
        设置
      </button>
    </TitleBar>
    <main class="main">
      <HomeView v-if="view.name === 'home'" @edit="view = { name: 'editor', projectId: $event }" @scan="view = { name: 'scan' }" @notify="showToast" />
      <ScanView v-else-if="view.name === 'scan'" @back="view = { name: 'home' }" @notify="showToast" />
      <EditorView v-else-if="view.name === 'editor'" :project-id="view.projectId" @back="view = { name: 'home' }" @notify="showToast" />
      <SettingsView v-else @notify="showToast" />
    </main>
    <Transition name="toast">
      <div v-if="toast" class="toast" :class="toastKind">{{ toast }}</div>
    </Transition>
  </div>

  <div v-else-if="loadError" class="boot">
    <div class="empty-state">
      <div class="empty-title">配置加载失败</div>
      <div class="empty-sub">{{ loadError }}</div>
      <button class="primary" @click="init">重试</button>
    </div>
  </div>
</template>

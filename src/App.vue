<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { load, config } from './store'
import type { View, ToastKind } from './types'
import TitleBar from './components/TitleBar.vue'
import HomeView from './views/HomeView.vue'
import EditorView from './views/EditorView.vue'
import SettingsView from './views/SettingsView.vue'

const view = ref<View>({ name: 'home' })
const toast = ref('')
const toastKind = ref<ToastKind>('ok')
let toastTimer: number | undefined

function showToast(msg: string, kind: ToastKind = 'ok') {
  toast.value = msg
  toastKind.value = kind
  clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => (toast.value = ''), 5000)
}

onMounted(async () => {
  await load()
})
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
      <HomeView v-if="view.name === 'home'" @edit="view = { name: 'editor', projectId: $event }" @notify="showToast" />
      <EditorView v-else-if="view.name === 'editor'" :project-id="view.projectId" @back="view = { name: 'home' }" @notify="showToast" />
      <SettingsView v-else @notify="showToast" />
    </main>
    <Transition name="toast">
      <div v-if="toast" class="toast" :class="toastKind">{{ toast }}</div>
    </Transition>
  </div>
</template>

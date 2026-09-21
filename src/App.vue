<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { load, config } from './store'
import { refreshStatuses } from './gitStore'
import type { View, ToastKind } from './types'
import TitleBar from './components/TitleBar.vue'
import SideBar from './components/SideBar.vue'
import HomeView from './views/HomeView.vue'
import GitView from './views/GitView.vue'
import EnvView from './views/EnvView.vue'
import ScanView from './views/ScanView.vue'
import EditorView from './views/EditorView.vue'
import SettingsView from './views/SettingsView.vue'

const view = ref<View>({ name: 'home' })
const toast = ref('')

const NAV_TITLES: Record<string, string> = {
  home: '项目',
  env: '环境',
  git: 'Git',
  settings: '设置',
  scan: '扫描工作区',
  editor: '编辑器',
}

const pageTitle = computed(() => `DevLaunch — ${NAV_TITLES[view.value.name] ?? 'DevLaunch'}`)

type NavTarget = 'home' | 'env' | 'git' | 'settings'

/** 静态路由表：用映射代替 `{ name: next } as View`——后者绕过了联合类型检查，
 *  将来给某个分支加必填字段（如 git 需要 projectId）时编译器不会提醒。 */
const NAV_ROUTES: Record<NavTarget, View> = {
  home: { name: 'home' },
  env: { name: 'env' },
  git: { name: 'git' },
  settings: { name: 'settings' },
}

function go(next: NavTarget) {
  view.value = NAV_ROUTES[next]
}

/** 编辑器以「项目一定存在」为前置条件（它内部用了非空断言），
 *  这个保证必须由这里给：项目可能在别处被删掉，那时不该白屏。 */
function projectExists(id: string): boolean {
  return !!config.value?.projects.some((p) => p.id === id)
}

const toastKind = ref<ToastKind>('ok')
const loadError = ref('')
let toastTimer: number | undefined
let unlisten: (() => void) | undefined
let focusTimer: number | undefined

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

function onWindowFocus() {
  clearTimeout(focusTimer)
  focusTimer = window.setTimeout(() => {
    refreshStatuses(config.value?.projects.map((p) => p.id) ?? [])
  }, 500)
}

onMounted(async () => {
  await init()
  unlisten = await listen<string>('launch-error', (e) => showToast(e.payload, 'err'))
  window.addEventListener('focus', onWindowFocus)
})

onUnmounted(() => {
  unlisten?.()
  window.removeEventListener('focus', onWindowFocus)
  clearTimeout(focusTimer)
  clearTimeout(toastTimer)
})
</script>

<template>
  <div class="app" v-if="config">
    <SideBar :active="view.name" @go="go" />
    <div class="content">
      <TitleBar :title="pageTitle" />
      <main class="main">
        <HomeView
          v-if="view.name === 'home'"
          @edit="view = { name: 'editor', projectId: $event }"
          @scan="view = { name: 'scan' }"
          @open-git="view = { name: 'git', projectId: $event }"
          @notify="showToast"
        />
        <GitView v-else-if="view.name === 'git'" :project-id="view.projectId" />
        <EnvView v-else-if="view.name === 'env'" :project-id="view.projectId" @notify="showToast" />
        <ScanView v-else-if="view.name === 'scan'" @back="view = { name: 'home' }" @notify="showToast" />
        <EditorView
          v-else-if="view.name === 'editor' && projectExists(view.projectId)"
          :project-id="view.projectId"
          @back="view = { name: 'home' }"
          @notify="showToast"
        />
        <div v-else-if="view.name === 'editor'" class="empty-state">
          <div class="empty-title">项目已不存在</div>
          <div class="empty-sub">它可能已在别处被删除</div>
          <button class="bordered" @click="go('home')">返回项目列表</button>
        </div>
        <SettingsView v-else @notify="showToast" />
      </main>
    </div>
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

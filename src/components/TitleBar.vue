<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'

const win = getCurrentWindow()

const isMax = ref(false)
let unlisten: (() => void) | undefined

onMounted(async () => {
  try {
    isMax.value = await win.isMaximized()
    unlisten = await win.onResized(async () => {
      isMax.value = await win.isMaximized()
    })
  } catch {
    isMax.value = false
  }
})

onUnmounted(() => unlisten?.())

function minimize() {
  win.minimize()
}

function toggleMaximize() {
  win.toggleMaximize()
}

function close() {
  win.hide()
}
</script>

<template>
  <header class="titlebar">
    <div class="brand" data-tauri-drag-region>
      <span class="brand-dot" />
      <span>DevLaunch</span>
      <span class="brand-cursor" />
    </div>
    <nav class="tb-nav">
      <slot />
    </nav>
    <div class="tb-spacer" data-tauri-drag-region />
    <div class="win-controls">
      <button class="win-btn" title="最小化" @click="minimize">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <button class="win-btn" :title="isMax ? '还原' : '最大化'" @click="toggleMaximize">
        <svg v-if="!isMax" width="10" height="10" viewBox="0 0 10 10">
          <rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" stroke-width="1" fill="none" />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 10 10">
          <rect x="0.5" y="2.5" width="7" height="7" stroke="currentColor" stroke-width="1" fill="none" />
          <path d="M2.5 2.5v-2h7v7h-2" stroke="currentColor" stroke-width="1" fill="none" />
        </svg>
      </button>
      <button class="win-btn close" title="隐藏到托盘" @click="close">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
  </header>
</template>

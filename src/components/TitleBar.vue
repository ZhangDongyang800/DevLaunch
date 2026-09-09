<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'

const win = getCurrentWindow()

function minimize() {
  win.minimize()
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
      <button class="win-btn close" title="隐藏到托盘" @click="close">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
  </header>
</template>

<script setup lang="ts">
import type { View } from '../types'

type Nav = 'home' | 'env' | 'git' | 'settings'

const props = defineProps<{ active: View['name'] }>()
const emit = defineEmits<{ (e: 'go', view: Nav): void }>()

const items: { key: Nav; label: string; hint: string }[] = [
  { key: 'home', label: '项目', hint: '项目与启动项' },
  { key: 'env', label: '环境', hint: '按任务的 worktree 环境' },
  { key: 'git', label: 'Git', hint: 'Changes / History' },
]
</script>

<template>
  <nav class="sidebar" aria-label="主导航">
    <div class="sb-logo" title="DevLaunch" data-tauri-drag-region>
      <svg width="22" height="22" viewBox="0 0 22 22" fill="none" aria-hidden="true">
        <rect x="1" y="1" width="20" height="20" rx="5" stroke="currentColor" stroke-width="1.5" />
        <path d="M8.5 7.2v7.6l6.2-3.8-6.2-3.8Z" fill="currentColor" />
      </svg>
    </div>

    <div class="sb-nav">
      <button
        v-for="it in items"
        :key="it.key"
        class="sb-btn"
        :class="{ active: props.active === it.key }"
        :title="it.hint"
        :aria-label="it.label"
        :aria-current="props.active === it.key ? 'page' : undefined"
        @click="emit('go', it.key)"
      >
        <svg v-if="it.key === 'home'" width="20" height="20" viewBox="0 0 22 22" fill="none" aria-hidden="true">
          <path
            d="M2.75 6.5c0-1.1.9-2 2-2h3.2c.6 0 1.15.3 1.5.75l1.05 1.4c.35.45.9.75 1.5.75h5.75c1.1 0 2 .9 2 2v7.6c0 1.1-.9 2-2 2H4.75c-1.1 0-2-.9-2-2v-11.7Z"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linejoin="round"
          />
        </svg>
        <svg v-else-if="it.key === 'env'" width="20" height="20" viewBox="0 0 22 22" fill="none" aria-hidden="true">
          <rect x="2.25" y="3.75" width="17.5" height="14.5" rx="2.5" stroke="currentColor" stroke-width="1.5" />
          <path d="M6 8.5l3 2.5-3 2.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          <path d="M11.5 14.5h4.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
        <svg v-else width="20" height="20" viewBox="0 0 22 22" fill="none" aria-hidden="true">
          <circle cx="6.25" cy="5.5" r="2.25" stroke="currentColor" stroke-width="1.5" />
          <circle cx="6.25" cy="16.5" r="2.25" stroke="currentColor" stroke-width="1.5" />
          <circle cx="15.75" cy="8.5" r="2.25" stroke="currentColor" stroke-width="1.5" />
          <path d="M6.25 7.75v6.5" stroke="currentColor" stroke-width="1.5" />
          <path
            d="M15.75 10.75c0 2.6-2.1 3.6-4.6 4.05-1.6.28-2.9.55-3.7 1.2"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
      </button>
    </div>

    <div class="sb-foot">
      <button
        class="sb-btn"
        :class="{ active: props.active === 'settings' }"
        title="设置"
        aria-label="设置"
        :aria-current="props.active === 'settings' ? 'page' : undefined"
        @click="emit('go', 'settings')"
      >
        <svg width="20" height="20" viewBox="0 0 22 22" fill="none" aria-hidden="true">
          <circle cx="11" cy="11" r="2.9" stroke="currentColor" stroke-width="1.5" />
          <path
            d="M9.15 3.35a6.9 6.9 0 0 1 3.7 0l.35 1.9a6.4 6.4 0 0 1 1.6.93l1.83-.66 1.85 3.2-1.48 1.27a6.4 6.4 0 0 1 0 1.86l1.48 1.27-1.85 3.2-1.83-.66c-.49.4-1.03.72-1.6.93l-.35 1.9a6.9 6.9 0 0 1-3.7 0l-.35-1.9a6.4 6.4 0 0 1-1.6-.93l-1.83.66-1.85-3.2 1.48-1.27a6.4 6.4 0 0 1 0-1.86L3.52 8.72l1.85-3.2 1.83.66c.49-.4 1.03-.72 1.6-.93l.35-1.9Z"
            stroke="currentColor"
            stroke-width="1.4"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    </div>
  </nav>
</template>

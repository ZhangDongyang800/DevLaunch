<script setup lang="ts">
import { computed } from 'vue'
import { config } from '../store'
import type { View } from '../types'
import LaunchMark from './LaunchMark.vue'

type Nav = 'home' | 'git' | 'settings'

const props = defineProps<{ active: View['name']; version: string; readonly: boolean }>()
const emit = defineEmits<{ (e: 'go', view: Nav): void }>()

const items: { key: Nav; label: string; hint: string }[] = [
  { key: 'home', label: '项目', hint: '项目与启动项' },
  { key: 'git', label: 'Git', hint: '变更与历史' },
  { key: 'settings', label: '设置', hint: '应用设置' },
]

const projectCount = computed(() => config.value?.projects.length ?? 0)
</script>

<template>
  <nav class="launch-rail" aria-label="主导航">
    <div class="rail-brand" title="DevLaunch" data-tauri-drag-region>
      <LaunchMark :size="22" />
      <span>
        <strong>DevLaunch</strong>
        <small>本地启动控制台</small>
      </span>
    </div>

    <div class="rail-nav">
      <button
        v-for="item in items"
        :key="item.key"
        :class="{ active: props.active === item.key }"
        :title="item.hint"
        :aria-label="item.label"
        :aria-current="props.active === item.key ? 'page' : undefined"
        @click="emit('go', item.key)"
      >
        <span class="rail-icon">
          <LaunchMark v-if="item.key === 'home'" :size="18" />
          <svg v-else-if="item.key === 'git'" width="18" height="18" viewBox="0 0 22 22" fill="none" aria-hidden="true">
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
          <svg v-else width="18" height="18" viewBox="0 0 22 22" fill="none" aria-hidden="true">
            <circle cx="11" cy="11" r="2.9" stroke="currentColor" stroke-width="1.5" />
            <path
              d="M9.15 3.35a6.9 6.9 0 0 1 3.7 0l.35 1.9a6.4 6.4 0 0 1 1.6.93l1.83-.66 1.85 3.2-1.48 1.27a6.4 6.4 0 0 1 0 1.86l1.48 1.27-1.85 3.2-1.83-.66c-.49.4-1.03.72-1.6.93l-.35 1.9a6.9 6.9 0 0 1-3.7 0l-.35-1.9a6.4 6.4 0 0 1-1.6-.93l-1.83.66-1.85-3.2 1.48-1.27a6.4 6.4 0 0 1 0-1.86L3.52 8.72l1.85-3.2 1.83.66c.49-.4 1.03-.72 1.6-.93l.35-1.9Z"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linejoin="round"
            />
          </svg>
        </span>
        <span>{{ item.label }}</span>
      </button>
    </div>

    <div class="rail-foot">
      <span>{{ projectCount }} 个项目已配置</span>
      <span>{{ props.readonly ? '配置只读' : '已就绪' }} · 本地无遥测</span>
      <span v-if="version" class="mono">v{{ version }}</span>
    </div>
  </nav>
</template>

import { ref, type Ref } from 'vue'
import type { AppConfig, ConfigStatus } from './types'
import { getConfig, getConfigStatus, saveConfig } from './api'

export const config: Ref<AppConfig | null> = ref(null)
export const configStatus: Ref<ConfigStatus> = ref({ blocked: false, reason: null, path: '' })

// 主题 = 一组 token 覆盖块（见 style.css 的 [data-theme]），只改配色不改布局。
export const THEMES = ['signal', 'graphite', 'indigo', 'amber'] as const
export type ThemeName = (typeof THEMES)[number]
const DEFAULT_THEME: ThemeName = 'signal'

// 当前生效主题；读配置前先用默认值，避免首帧闪色
export const theme: Ref<ThemeName> = ref(DEFAULT_THEME)

export function applyTheme(name: string | null | undefined): void {
  const next = (THEMES as readonly string[]).includes(name ?? '')
    ? (name as ThemeName)
    : DEFAULT_THEME
  theme.value = next
  document.documentElement.dataset.theme = next
}

export async function load(): Promise<void> {
  const [loaded, status] = await Promise.all([getConfig(), getConfigStatus()])
  config.value = loaded
  configStatus.value = status
  applyTheme(config.value?.settings.theme)
}

export async function persist(): Promise<void> {
  if (config.value) {
    await saveConfig(JSON.parse(JSON.stringify(config.value)))
  }
}

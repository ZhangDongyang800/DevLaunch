import { ref, type Ref } from 'vue'
import type { AppConfig } from './types'
import { getConfig, saveConfig } from './api'

export const config: Ref<AppConfig | null> = ref(null)

export async function load(): Promise<void> {
  config.value = await getConfig()
}

export async function persist(): Promise<void> {
  if (config.value) {
    await saveConfig(JSON.parse(JSON.stringify(config.value)))
  }
}

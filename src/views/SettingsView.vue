<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { config, persist } from '../store'
import { exportConfigTo, getAutostart, getConfig, importConfigFrom, setAutostart } from '../api'

const emit = defineEmits<{ notify: [msg: string] }>()

const autostart = ref(false)

onMounted(async () => {
  try {
    autostart.value = await getAutostart()
  } catch {
    autostart.value = false
  }
})

async function toggleAutostart() {
  try {
    await setAutostart(autostart.value)
    emit('notify', autostart.value ? '已开启开机自启' : '已关闭开机自启')
  } catch (e) {
    autostart.value = !autostart.value
    emit('notify', `设置失败：${e}`)
  }
}

async function saveTimeout() {
  try {
    await persist()
    emit('notify', '默认超时已保存')
  } catch (e) {
    emit('notify', `保存失败：${e}`)
  }
}

async function doExport() {
  const { save } = await import('@tauri-apps/plugin-dialog')
  const path = await save({
    defaultPath: 'devlaunch-config.json',
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (!path) return
  try {
    await exportConfigTo(path)
    emit('notify', '配置已导出')
  } catch (e) {
    emit('notify', `导出失败：${e}`)
  }
}

async function doImport() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    multiple: false,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (typeof picked !== 'string') return
  try {
    await importConfigFrom(picked)
    config.value = await getConfig()
    emit('notify', '配置已导入')
  } catch (e) {
    emit('notify', `导入失败：${e}`)
  }
}
</script>

<template>
  <div class="settings" v-if="config">
    <h1>设置</h1>

    <div class="group-card">
      <label>默认就绪超时（秒）
        <input type="number" v-model.number="config.settings.readyTimeoutSec" min="1" @change="saveTimeout" />
      </label>
    </div>

    <div class="group-card row">
      <input type="checkbox" v-model="autostart" @change="toggleAutostart" id="autostart" />
      <label for="autostart" style="flex-direction: row">开机自动启动 DevLaunch（托盘常驻）</label>
    </div>

    <div class="group-card row">
      <button @click="doExport">导出配置</button>
      <button @click="doImport">导入配置</button>
    </div>

    <p class="hint">配置文件位置：%APPDATA%\com.devlaunch.app\config.json</p>
  </div>
</template>

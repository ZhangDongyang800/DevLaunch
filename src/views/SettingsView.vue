<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { config } from '../store'
import { exportConfigTo, getAutostart, getConfig, importConfigFrom, setAutostart } from '../api'

const emit = defineEmits<{ notify: [msg: string, kind?: 'ok' | 'err'] }>()

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
    emit('notify', `设置失败：${e}`, 'err')
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
    emit('notify', `导出失败：${e}`, 'err')
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
    emit('notify', `导入失败：${e}`, 'err')
  }
}
</script>

<template>
  <div class="settings" v-if="config">
    <div class="home-head">
      <h1>设置</h1>
      <span class="home-count mono">SETTINGS</span>
    </div>

    <div class="list">
      <div class="list-row">
        <div class="set-info">
          <div class="set-title">开机自动启动</div>
          <div class="set-sub">开机后 DevLaunch 常驻托盘，随时一键启动项目</div>
        </div>
        <label class="switch">
          <input type="checkbox" v-model="autostart" @change="toggleAutostart" />
          <span class="slider" />
        </label>
      </div>

      <div class="list-row">
        <div class="set-info">
          <div class="set-title">配置备份</div>
          <div class="set-sub">导出 JSON 备份，或从备份文件导入恢复</div>
        </div>
        <button class="ghost" @click="doExport">导出</button>
        <button class="ghost" @click="doImport">导入</button>
      </div>
    </div>

    <p class="hint mono">%APPDATA%\com.devlaunch.app\config.json</p>
  </div>
</template>

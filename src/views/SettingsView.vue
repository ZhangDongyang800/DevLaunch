<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { config, persist } from '../store'
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

async function saveTimeout() {
  try {
    await persist()
    emit('notify', '默认超时已保存')
  } catch (e) {
    emit('notify', `保存失败：${e}`, 'err')
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
    <h1 style="margin-bottom: 16px">设置</h1>

    <div class="set-row">
      <div class="set-info">
        <div class="set-title">开机自动启动</div>
        <div class="set-sub">开机后 DevLaunch 常驻托盘，随时一键启动项目</div>
      </div>
      <label class="switch">
        <input type="checkbox" v-model="autostart" @change="toggleAutostart" />
        <span class="slider" />
      </label>
    </div>

    <div class="set-row">
      <div class="set-info">
        <div class="set-title">默认就绪超时</div>
        <div class="set-sub">步骤等待端口 / 进程就绪的兜底秒数</div>
      </div>
      <input
        type="number"
        class="mono"
        style="width: 90px"
        v-model.number="config.settings.readyTimeoutSec"
        min="1"
        @change="saveTimeout"
      />
    </div>

    <div class="set-row">
      <div class="set-info">
        <div class="set-title">配置备份</div>
        <div class="set-sub">导出 JSON 备份，或从备份文件导入恢复</div>
      </div>
      <button @click="doExport">导出</button>
      <button @click="doImport">导入</button>
    </div>

    <p class="hint">配置文件位置：%APPDATA%\com.devlaunch.app\config.json</p>
  </div>
</template>

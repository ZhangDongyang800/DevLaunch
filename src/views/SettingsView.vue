<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { applyTheme, config, persist, THEMES } from '../store'
import { exportConfigTo, getAutostart, getConfig, getGitInfo, importConfigFrom, setAutostart, setHotkey } from '../api'
import type { GitInfo } from '../types'

const emit = defineEmits<{ notify: [msg: string, kind?: 'ok' | 'err'] }>()

const autostart = ref(false)
const gitPath = ref('')
const gitInfo = ref<GitInfo>({ configured: null, resolved: null })

const THEME_LABELS: Record<string, string> = {
  signal: 'Signal · 黑 + 信号绿（默认）',
  graphite: '石墨青 · 冷灰 + 青',
  indigo: '靛紫 · 紫黑 + 紫',
  amber: '琥珀棕 · 暖黑 + 金',
}

async function onThemeChange(e: Event) {
  if (!config.value) return
  const select = e.target as HTMLSelectElement
  const next = select.value
  const previous = config.value.settings.theme
  if (next === previous) return
  config.value.settings.theme = next
  applyTheme(next)
  try {
    await persist()
    emit('notify', '主题已切换')
  } catch (err) {
    config.value!.settings.theme = previous
    applyTheme(previous)
    select.value = previous
    emit('notify', `保存失败：${err}`, 'err')
  }
}

onMounted(async () => {
  try {
    autostart.value = await getAutostart()
  } catch {
    autostart.value = false
  }
  await loadGitInfo()
})

async function loadGitInfo() {
  try {
    gitInfo.value = await getGitInfo()
  } catch {
    gitInfo.value = { configured: null, resolved: null }
  }
  gitPath.value = config.value?.settings.gitPath ?? ''
}

async function saveGitPath(path: string) {
  if (!config.value) return
  const trimmed = path.trim()
  config.value.settings.gitPath = trimmed ? trimmed : null
  try {
    await persist()
    await loadGitInfo()
    emit('notify', gitInfo.value.resolved ? '已找到 git.exe' : '未找到 git.exe，请确认路径或留空自动检测')
  } catch (e) {
    emit('notify', `保存失败：${e}`, 'err')
  }
}

function onGitPathChange(e: Event) {
  void saveGitPath((e.target as HTMLInputElement).value)
}

async function browseGit() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ multiple: false, filters: [{ name: 'git.exe', extensions: ['exe'] }] })
  if (typeof picked === 'string') {
    gitPath.value = picked
    await saveGitPath(picked)
  }
}

function clearGit() {
  gitPath.value = ''
  void saveGitPath('')
}

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
  const { open, confirm } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    multiple: false,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (typeof picked !== 'string') return
  const ok = await confirm('导入会覆盖当前全部项目配置（现有配置会自动备份），是否继续？', {
    title: '导入配置',
    kind: 'warning',
  })
  if (!ok) return
  try {
    await importConfigFrom(picked)
    config.value = await getConfig()
    emit('notify', '配置已导入')
  } catch (e) {
    emit('notify', `导入失败：${e}`, 'err')
  }
}

const recording = ref(false)
const preview = ref('')

const hotkey = computed(() => config.value?.settings.hotkey ?? '')

function keyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3)
  if (/^Digit[0-9]$/.test(code)) return code.slice(5)
  if (/^F([1-9]|1[0-2])$/.test(code)) return code
  if (code === 'Space') return 'Space'
  return null
}

function comboFromEvent(e: KeyboardEvent): string | null {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Win')
  if (parts.length === 0) return null
  const key = keyFromCode(e.code)
  if (!key) return null
  parts.push(key)
  return parts.join('+')
}

function onRecordKeydown(e: KeyboardEvent) {
  e.preventDefault()
  e.stopPropagation()
  if (e.repeat) return
  if (e.key === 'Escape') {
    stopRecording()
    return
  }
  const combo = comboFromEvent(e)
  if (!combo) return
  preview.value = combo
  void applyHotkey(combo)
}

function startRecording() {
  recording.value = true
  preview.value = ''
  window.addEventListener('keydown', onRecordKeydown, true)
}

function stopRecording() {
  recording.value = false
  preview.value = ''
  window.removeEventListener('keydown', onRecordKeydown, true)
}

async function applyHotkey(combo: string) {
  stopRecording()
  try {
    await setHotkey(combo)
    config.value = await getConfig()
    emit('notify', `全局快捷键已更新为 ${combo}`)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

function resetHotkey() {
  void applyHotkey('Ctrl+Alt+D')
}

onUnmounted(() => window.removeEventListener('keydown', onRecordKeydown, true))
</script>

<template>
  <div class="settings" v-if="config">
    <div class="page-head">
      <h1>设置</h1>
      <span class="page-meta">SETTINGS</span>
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
          <div class="set-title">全局快捷键</div>
          <div class="set-sub">
            {{ recording ? '按下新的组合键（至少一个修饰键，Esc 取消）' : '唤起搜索面板；被其他程序占用时会提示' }}
          </div>
        </div>
        <input class="inline mono hotkey-input" readonly :value="recording ? preview || '监听中…' : hotkey" />
        <button v-if="!recording" class="ghost" @click="startRecording">录制</button>
        <button v-else class="ghost" @click="stopRecording">取消</button>
        <button class="ghost" @click="resetHotkey">恢复默认</button>
      </div>

      <div class="list-row">
        <div class="set-info">
          <div class="set-title">Git 可执行文件</div>
          <div class="set-sub">
            {{ gitInfo.resolved ? `已找到：${gitInfo.resolved}` : '未找到；留空则自动检测 PATH / Program Files' }}
          </div>
        </div>
        <input
          class="inline mono gitpath-input"
          :value="gitPath"
          placeholder="自动检测"
          spellcheck="false"
          @change="onGitPathChange"
        />
        <button class="ghost" @click="browseGit">选择…</button>
        <button class="ghost" @click="clearGit">清除</button>
      </div>

      <div class="list-row">
        <div class="set-info">
          <div class="set-title">主题</div>
          <div class="set-sub">只换配色（底色 / 边框 / 强调色），不改布局与字号；搜索面板同步生效，不写入 devlaunch.json</div>
        </div>
        <select class="inline mono theme-select" :value="config.settings.theme" @change="onThemeChange">
          <option v-for="t in THEMES" :key="t" :value="t">{{ THEME_LABELS[t] }}</option>
        </select>
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

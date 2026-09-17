<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { gitCommitDetail } from '../api'
import type { CommitDetail } from '../types'
import {
  cherryPickCommit,
  loadFileHistory,
  loadLog,
  loadLogFiltered,
  logCache,
  openFileInApp,
  resetTo,
  revertCommit,
} from '../gitStore'
import GitGraph from './GitGraph.vue'
import GitDiff from './GitDiff.vue'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ notify: [msg: string, kind?: 'ok' | 'err'] }>()

const baseRows = computed(() => logCache.value[props.projectId] ?? [])
const historyRows = ref<import('../types').GraphRow[] | null>(null)
const historyPath = ref('')
const rows = computed(() => historyRows.value ?? baseRows.value)

const query = ref('')
const author = ref('')
const filtering = computed(() => !!query.value.trim() || !!author.value.trim())

const selected = ref('')
const detail = ref<CommitDetail | null>(null)
const selectedFile = ref(0)
const loading = ref(false)
const error = ref('')
let seq = 0
let loadingMore = false
let exhausted = false

const currentFile = computed(() => detail.value?.files[selectedFile.value] ?? null)

const menu = ref<{ x: number; y: number; hash: string } | null>(null)
const fileMenu = ref<{ x: number; y: number; path: string } | null>(null)

onMounted(() => reload())

async function reload() {
  historyRows.value = null
  historyPath.value = ''
  selected.value = ''
  detail.value = null
  exhausted = false
  try {
    if (filtering.value) await loadLogFiltered(props.projectId, true, query.value, author.value)
    else await loadLog(props.projectId, true)
  } catch (e) {
    error.value = `${e}`
  }
}

async function onScroll(e: Event) {
  const el = e.target as HTMLElement
  if (historyRows.value || loadingMore || exhausted) return
  if (el.scrollTop + el.clientHeight < el.scrollHeight - 40) return
  const before = rows.value.length
  if (before === 0) return
  loadingMore = true
  try {
    await loadLogFiltered(props.projectId, false, query.value, author.value)
    if (rows.value.length === before) exhausted = true
  } catch (e) {
    error.value = `${e}`
  } finally {
    loadingMore = false
  }
}

async function select(hash: string) {
  const mine = ++seq
  selected.value = hash
  selectedFile.value = 0
  loading.value = true
  error.value = ''
  detail.value = null
  try {
    const d = await gitCommitDetail(props.projectId, hash)
    if (mine !== seq) return
    detail.value = d
  } catch (e) {
    if (mine !== seq) return
    error.value = `${e}`
  } finally {
    if (mine === seq) loading.value = false
  }
}

function relTime(iso: string): string {
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return iso
  const d = Date.now() - t
  const day = 86_400_000
  if (d < 60_000) return '刚刚'
  if (d < 3_600_000) return `${Math.floor(d / 60_000)} 分钟前`
  if (d < day) return `${Math.floor(d / 3_600_000)} 小时前`
  if (d < day * 30) return `${Math.floor(d / day)} 天前`
  return new Date(t).toLocaleDateString()
}

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    emit('notify', '已复制 SHA')
    return
  } catch {
    const ta = document.createElement('textarea')
    ta.value = text
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
    emit('notify', '已复制 SHA')
  }
}

function openCommitMenu(e: MouseEvent, hash: string) {
  menu.value = { x: e.clientX, y: e.clientY, hash }
  fileMenu.value = null
}

function openFileMenu(e: MouseEvent, path: string) {
  fileMenu.value = { x: e.clientX, y: e.clientY, path }
  menu.value = null
}

function closeMenus() {
  menu.value = null
  fileMenu.value = null
}

async function runCommitOp(op: 'revert' | 'cherry' | 'soft' | 'mixed') {
  const hash = menu.value?.hash
  closeMenus()
  if (!hash) return
  const { confirm } = await import('@tauri-apps/plugin-dialog')
  const labels = { revert: 'Revert 该提交（新建反向提交）', cherry: 'Cherry-pick 该提交到当前分支', soft: 'Reset --soft 到该提交', mixed: 'Reset --mixed 到该提交' }
  const ok = await confirm(`${labels[op]}？\n${hash}`, { title: '确认操作', kind: 'warning' })
  if (!ok) return
  try {
    if (op === 'revert') await revertCommit(props.projectId, hash)
    else if (op === 'cherry') await cherryPickCommit(props.projectId, hash)
    else await resetTo(props.projectId, hash, op)
    emit('notify', '操作完成')
    await reload()
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function openSelectedFile() {
  const path = fileMenu.value?.path
  closeMenus()
  if (!path) return
  try {
    await openFileInApp(props.projectId, path)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function showFileHistory() {
  const path = fileMenu.value?.path
  closeMenus()
  if (!path) return
  try {
    historyRows.value = await loadFileHistory(props.projectId, path)
    historyPath.value = path
    selected.value = ''
    detail.value = null
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

function backToAll() {
  historyRows.value = null
  historyPath.value = ''
}
</script>

<template>
  <div class="git-history" @click="closeMenus">
    <div class="git-history-log">
      <div class="history-filters">
        <input v-model="query" class="inline mono" placeholder="搜索提交信息…" spellcheck="false" @keydown.enter.prevent="reload" />
        <input v-model="author" class="inline mono" placeholder="作者" spellcheck="false" @keydown.enter.prevent="reload" />
        <button class="ghost" @click="reload">筛选</button>
        <button v-if="filtering || historyPath" class="ghost" @click="query = ''; author = ''; reload()">清除</button>
      </div>
      <div v-if="historyPath" class="file-history-head mono">
        <button class="ghost" @click="backToAll">← 全部历史</button>
        <span class="gt-subject">{{ historyPath }}</span>
      </div>
      <div class="git-col git-commits">
        <div class="gc-body git-log-scroll" @scroll="onScroll">
          <GitGraph :rows="rows" :selected="selected" />
          <div class="git-log-list">
            <div
              v-for="r in rows"
              :key="r.commit.hash"
              class="gt-commit"
              :class="{ active: selected === r.commit.hash }"
              @click="select(r.commit.hash)"
              @contextmenu.prevent="openCommitMenu($event, r.commit.hash)"
            >
              <span class="gt-hash mono">{{ r.commit.short }}</span>
              <span v-for="ref in r.commit.refs" :key="ref" class="gt-ref mono" :class="{ head: ref.includes('HEAD') }">{{ ref }}</span>
              <span class="gt-subject">{{ r.commit.subject }}</span>
              <span class="gt-author">{{ r.commit.author }}</span>
              <span class="gt-time">{{ relTime(r.commit.date) }}</span>
            </div>
            <div v-if="rows.length === 0" class="gc-empty">{{ filtering ? '无匹配提交' : '还没有提交' }}</div>
          </div>
        </div>
      </div>
    </div>

    <div class="git-col git-diff">
      <div class="gc-head">
        差异<span v-if="detail" class="mono"> · {{ detail.files.length }} 文件 · <span class="diff-stat-add">+{{ detail.additions }}</span> <span class="diff-stat-del">−{{ detail.deletions }}</span></span>
        <span v-if="error" class="gp-error"> · {{ error }}</span>
      </div>
      <div v-if="detail && detail.files.length > 0" class="commit-files mono">
        <div
          v-for="(f, fi) in detail.files"
          :key="f.path"
          class="commit-file"
          :class="{ active: fi === selectedFile }"
          @click="selectedFile = fi"
          @dblclick="openFileInApp(projectId, f.path).catch((e) => emit('notify', `${e}`, 'err'))"
          @contextmenu.prevent="openFileMenu($event, f.path)"
        >
          <span class="file-name">{{ f.path }}</span>
          <span class="diff-stat-add">+{{ f.additions }}</span>
          <span class="diff-stat-del">−{{ f.deletions }}</span>
        </div>
      </div>
      <GitDiff :file="currentFile" :loading="loading" />
    </div>

    <div v-if="menu" class="ctx-menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }" @click.stop>
      <button class="ctx-item" @click="copyText(menu.hash)">复制 SHA</button>
      <button class="ctx-item" @click="runCommitOp('revert')">Revert 该提交</button>
      <button class="ctx-item" @click="runCommitOp('cherry')">Cherry-pick 到当前分支</button>
      <button class="ctx-item" @click="runCommitOp('soft')">Reset --soft 到此</button>
      <button class="ctx-item" @click="runCommitOp('mixed')">Reset --mixed 到此</button>
    </div>

    <div v-if="fileMenu" class="ctx-menu" :style="{ left: fileMenu.x + 'px', top: fileMenu.y + 'px' }" @click.stop>
      <button class="ctx-item" @click="openSelectedFile">打开文件</button>
      <button class="ctx-item" @click="showFileHistory">查看文件历史</button>
    </div>
  </div>
</template>

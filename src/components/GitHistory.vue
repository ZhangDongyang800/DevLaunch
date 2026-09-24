<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { positionContextMenu, relTime } from '../utils'
import { gitCommitDetail } from '../api'
import type { CommitDetail } from '../types'
import {
  busy,
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
// 已**应用**的筛选条件。输入框只是草稿：只有按回车或点「筛选」才生效。
// 否则"输入了词但没回车就往下滚"会用新 query 配旧的 skip 去追加，
// 把筛选结果接到未筛选列表后面——列表出现重复、空洞、`exhausted` 判定也失真。
const appliedQuery = ref('')
const appliedAuthor = ref('')
const filtering = computed(() => !!appliedQuery.value || !!appliedAuthor.value)

const selected = ref('')
const detail = ref<CommitDetail | null>(null)
const selectedFile = ref(0)
const loading = ref(false)
const error = ref('')
let seq = 0
let loadingMore = false
let exhausted = false

const currentFile = computed(() => detail.value?.files[selectedFile.value] ?? null)

type MenuAnchor = { left: number; top: number; right: number; bottom: number }
type CommitMenu = { x: number; y: number; hash: string; anchor: MenuAnchor }
type FileMenu = { x: number; y: number; path: string; anchor: MenuAnchor }

const menu = ref<CommitMenu | null>(null)
const fileMenu = ref<FileMenu | null>(null)
const menuRef = ref<HTMLElement>()
const fileMenuRef = ref<HTMLElement>()
let menuReturnFocus: HTMLElement | null = null

onMounted(() => reload())

// 顶栏换仓库时本组件不会重建，必须自己把上一个仓库的选中项/详情/筛选清掉。
watch(
  () => props.projectId,
  () => {
    seq++
    query.value = ''
    author.value = ''
    appliedQuery.value = ''
    appliedAuthor.value = ''
    selected.value = ''
    detail.value = null
    loading.value = false
    error.value = ''
    void reload()
  },
)

/** 把输入框的草稿提交为生效的筛选条件。 */
function applyFilters() {
  appliedQuery.value = query.value.trim()
  appliedAuthor.value = author.value.trim()
  void reload()
}

function clearFilters() {
  query.value = ''
  author.value = ''
  appliedQuery.value = ''
  appliedAuthor.value = ''
  void reload()
}

async function reload() {
  historyRows.value = null
  historyPath.value = ''
  selected.value = ''
  detail.value = null
  error.value = ''
  exhausted = false
  try {
    if (filtering.value) await loadLogFiltered(props.projectId, true, appliedQuery.value, appliedAuthor.value)
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
    // 用**已应用**的条件，保证 skip 与列表实际来源一致
    await loadLogFiltered(props.projectId, false, appliedQuery.value, appliedAuthor.value)
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

function positionMenu(element: HTMLElement, point: { x: number; y: number }, anchor: MenuAnchor) {
  const rect = element.getBoundingClientRect()
  const placed = positionContextMenu(
    point,
    anchor,
    { width: rect.width, height: rect.height },
    { width: window.innerWidth, height: window.innerHeight },
  )
  element.style.left = `${placed.x}px`
  element.style.top = `${placed.y}px`
}

function openCommitMenu(e: MouseEvent | KeyboardEvent, hash: string) {
  const target = e.currentTarget as HTMLElement
  const rect = target.getBoundingClientRect()
  const point = e instanceof MouseEvent ? { x: e.clientX, y: e.clientY } : { x: rect.left, y: rect.bottom }
  menu.value = { ...point, hash, anchor: rect }
  fileMenu.value = null
  menuReturnFocus = target
  void nextTick(() => {
    if (menuRef.value) positionMenu(menuRef.value, point, rect)
    menuRef.value?.querySelector('button')?.focus()
  })
}

function openFileMenu(e: MouseEvent | KeyboardEvent, path: string) {
  const target = e.currentTarget as HTMLElement
  const rect = target.getBoundingClientRect()
  const point = e instanceof MouseEvent ? { x: e.clientX, y: e.clientY } : { x: rect.left, y: rect.bottom }
  fileMenu.value = { ...point, path, anchor: rect }
  menu.value = null
  menuReturnFocus = target
  void nextTick(() => {
    if (fileMenuRef.value) positionMenu(fileMenuRef.value, point, rect)
    fileMenuRef.value?.querySelector('button')?.focus()
  })
}

function closeMenus(restoreFocus = false) {
  menu.value = null
  fileMenu.value = null
  const target = menuReturnFocus
  menuReturnFocus = null
  if (restoreFocus && target) void nextTick(() => target.focus())
}

function dismissMenus() {
  closeMenus()
}

async function copyMenuSha() {
  const hash = menu.value?.hash
  closeMenus(true)
  if (hash) await copyText(hash)
}

async function runCommitOp(op: 'revert' | 'cherry' | 'soft' | 'mixed') {
  const hash = menu.value?.hash
  closeMenus(true)
  if (!hash) return
  if (busy.value) {
    emit('notify', '已有 Git 操作进行中，请稍候', 'err')
    return
  }
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
  closeMenus(true)
  if (!path) return
  try {
    await openFileInApp(props.projectId, path)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function showFileHistory() {
  const path = fileMenu.value?.path
  closeMenus(true)
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
  <div class="git-history" @click="dismissMenus" @keydown.esc.stop="closeMenus(true)">
    <div class="git-history-log">
      <div class="history-filters">
        <input v-model="query" class="inline mono" placeholder="搜索提交信息…" aria-label="搜索提交信息" spellcheck="false" @keydown.enter.prevent="applyFilters" />
        <input v-model="author" class="inline mono" placeholder="作者" aria-label="筛选作者" spellcheck="false" @keydown.enter.prevent="applyFilters" />
        <button class="ghost" @click="applyFilters">筛选</button>
        <button v-if="filtering || historyPath" class="ghost" @click="clearFilters">清除</button>
      </div>
      <div v-if="historyPath" class="file-history-head mono">
        <button class="ghost" @click="backToAll">← 全部历史</button>
        <span class="gt-subject">{{ historyPath }}</span>
      </div>
      <div class="git-col git-commits">
        <div class="gc-body git-log-scroll" @scroll="onScroll">
          <GitGraph :rows="rows" :selected="selected" />
          <div class="git-log-list">
            <button
              v-for="r in rows"
              :key="r.commit.hash"
              type="button"
              class="gt-commit"
              :class="{ active: selected === r.commit.hash }"
              :aria-label="`查看提交 ${r.commit.short} ${r.commit.subject}`"
              aria-haspopup="menu"
              @click="select(r.commit.hash)"
              @contextmenu.prevent="openCommitMenu($event, r.commit.hash)"
              @keydown.shift.f10.prevent="openCommitMenu($event, r.commit.hash)"
              @keydown.contextmenu.prevent="openCommitMenu($event, r.commit.hash)"
            >
              <span class="gt-hash mono">{{ r.commit.short }}</span>
              <span v-for="ref in r.commit.refs" :key="ref" class="gt-ref mono" :class="{ head: ref.includes('HEAD') }">{{ ref }}</span>
              <span class="gt-subject">{{ r.commit.subject }}</span>
              <span class="gt-author">{{ r.commit.author }}</span>
              <span class="gt-time">{{ relTime(r.commit.date) }}</span>
            </button>
            <div v-if="loadingMore" class="gc-empty">正在加载更多提交…</div>
            <div v-else-if="exhausted" class="gc-empty">已到最早记录</div>
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
        <button
          v-for="(f, fi) in detail.files"
          :key="f.path"
          type="button"
          class="commit-file"
          :class="{ active: fi === selectedFile }"
          :aria-label="`查看文件差异 ${f.path}`"
          aria-haspopup="menu"
          @click="selectedFile = fi"
          @dblclick="openFileInApp(projectId, f.path).catch((e) => emit('notify', `${e}`, 'err'))"
          @contextmenu.prevent="openFileMenu($event, f.path)"
          @keydown.shift.f10.prevent="openFileMenu($event, f.path)"
          @keydown.contextmenu.prevent="openFileMenu($event, f.path)"
        >
          <span class="file-name">{{ f.path }}</span>
          <span class="diff-stat-add">+{{ f.additions }}</span>
          <span class="diff-stat-del">−{{ f.deletions }}</span>
        </button>
      </div>
      <GitDiff :file="currentFile" :loading="loading" :project-id="projectId" :hash="selected" />
    </div>

    <div
      v-if="menu"
      ref="menuRef"
      class="ctx-menu"
      role="menu"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
    >
      <button class="ctx-item" role="menuitem" @click="copyMenuSha">复制 SHA</button>
      <button class="ctx-item" role="menuitem" @click="runCommitOp('revert')">Revert 该提交</button>
      <button class="ctx-item" role="menuitem" @click="runCommitOp('cherry')">Cherry-pick 到当前分支</button>
      <button class="ctx-item" role="menuitem" @click="runCommitOp('soft')">Reset --soft 到此</button>
      <button class="ctx-item" role="menuitem" @click="runCommitOp('mixed')">Reset --mixed 到此</button>
    </div>

    <div
      v-if="fileMenu"
      ref="fileMenuRef"
      class="ctx-menu"
      role="menu"
      :style="{ left: fileMenu.x + 'px', top: fileMenu.y + 'px' }"
      @click.stop
    >
      <button class="ctx-item" role="menuitem" @click="openSelectedFile">打开文件</button>
      <button class="ctx-item" role="menuitem" @click="showFileHistory">查看文件历史</button>
    </div>
  </div>
</template>

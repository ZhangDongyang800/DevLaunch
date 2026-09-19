<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { gitFileDiff } from '../api'
import type { FileChange, FileDiff } from '../types'
import { busy, discard, stage, statuses, unstage } from '../gitStore'
import { buildTree, type TreeRow } from '../gitTree'
import GitDiff from './GitDiff.vue'
import GitCommitBox from './GitCommitBox.vue'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ notify: [msg: string, kind?: 'ok' | 'err'] }>()

const status = computed(() => statuses.value[props.projectId])
const files = computed<FileChange[]>(() => status.value?.files ?? [])
const stagedFiles = computed(() =>
  files.value.filter((f) => f.index !== '.' && f.index !== '?' && f.index !== '!' && f.index !== ' '),
)
const unstagedFiles = computed(() =>
  files.value.filter((f) => f.worktree !== '.' && f.worktree !== '?' && f.worktree !== '!' && f.worktree !== ' '),
)
const untrackedFiles = computed(() => files.value.filter((f) => f.index === '?'))
const fileMap = computed(() => new Map(files.value.map((f) => [f.path, f])))

const collapsedStaged = ref(new Set<string>())
const collapsedUnstaged = ref(new Set<string>())
const stagedRows = computed<TreeRow[]>(() => buildTree(stagedFiles.value.map((f) => f.path), collapsedStaged.value))
const unstagedRows = computed<TreeRow[]>(() =>
  buildTree([...unstagedFiles.value, ...untrackedFiles.value].map((f) => f.path), collapsedUnstaged.value),
)

const selPath = ref('')
const selStaged = ref(false)
const diff = ref<FileDiff | null>(null)
const loading = ref(false)
const error = ref('')
const ignoreWhitespace = ref(false)
const fullContext = ref(false)
// 原生 checkbox 的勾选态存在 DOM 里，Vue 不会因为「值没变」去纠正它；
// 每次写完 +1 强制重建复选框，失败时不会留下与数据不符的勾选状态。
const rev = ref(0)
// 快速连点时后到的旧响应不能覆盖先到的新响应。
let diffSeq = 0

watch(() => props.projectId, reset)

function reset() {
  selPath.value = ''
  selStaged.value = false
  diff.value = null
  error.value = ''
  ignoreWhitespace.value = false
  fullContext.value = false
  diffSeq++
}

async function openFile(path: string, staged: boolean) {
  const seq = ++diffSeq
  selPath.value = path
  selStaged.value = staged
  loading.value = true
  error.value = ''
  diff.value = null
  try {
    const got = await gitFileDiff(props.projectId, path, staged, ignoreWhitespace.value, fullContext.value)
    if (seq !== diffSeq) return
    diff.value = got
  } catch (e) {
    if (seq !== diffSeq) return
    error.value = `${e}`
  } finally {
    if (seq === diffSeq) loading.value = false
  }
}

async function reload() {
  if (selPath.value) await openFile(selPath.value, selStaged.value)
}

function toggleWhitespace() {
  ignoreWhitespace.value = !ignoreWhitespace.value
  void reload()
}

function toggleFullContext() {
  fullContext.value = !fullContext.value
  void reload()
}

function toggleCollapse(set: Set<string>, path: string): Set<string> {
  const next = new Set(set)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  return next
}

async function action(fn: () => Promise<void>) {
  try {
    await fn()
  } catch (e) {
    emit('notify', `${e}`, 'err')
  } finally {
    rev.value++
  }
}

async function stageAll() {
  const paths = [...unstagedFiles.value, ...untrackedFiles.value].map((f) => f.path)
  if (paths.length) await action(() => stage(props.projectId, paths))
}

async function unstageAll() {
  const paths = stagedFiles.value.map((f) => f.path)
  if (paths.length) await action(() => unstage(props.projectId, paths))
}

async function discardFile(path: string) {
  const { confirm } = await import('@tauri-apps/plugin-dialog')
  const ok = await confirm(`将丢弃 ${path} 的未提交修改，不可恢复。继续？`, { title: '丢弃修改', kind: 'warning' })
  if (ok) await action(() => discard(props.projectId, [path]))
}

function statusLetter(f: FileChange): string {
  const c = f.index !== '.' && f.index !== ' ' ? f.index : f.worktree
  return c
}
</script>

<template>
  <div class="git-changes">
    <div class="git-changes-lists">
      <div class="file-section">
        <div class="fs-head">
          <span>已暂存 ({{ stagedFiles.length }})</span>
          <span class="v-spacer" />
          <button class="ghost" :disabled="busy || stagedFiles.length === 0" @click="unstageAll">全部取消暂存</button>
        </div>
        <div class="fs-body">
          <template v-for="row in stagedRows" :key="'s' + row.path">
            <div
              v-if="row.isDir"
              class="tree-dir"
              :style="{ paddingLeft: 8 + row.indent * 14 + 'px' }"
              @click="collapsedStaged = toggleCollapse(collapsedStaged, row.path)"
            >
              {{ collapsedStaged.has(row.path) ? '▸' : '▾' }} {{ row.name }}
            </div>
            <div
              v-else
              class="file-row"
              :class="{ active: selPath === row.path && selStaged }"
              :style="{ paddingLeft: 8 + row.indent * 14 + 'px' }"
              @click="openFile(row.path, true)"
            >
              <input
                type="checkbox"
                checked
                :key="'scb' + row.path + rev"
                :disabled="busy"
                @click.stop="action(() => unstage(projectId, [row.path]))"
              />
              <span class="file-status mono">{{ fileMap.get(row.path) ? statusLetter(fileMap.get(row.path)!) : 'M' }}</span>
              <span class="file-name mono">{{ row.name }}</span>
            </div>
          </template>
          <div v-if="stagedFiles.length === 0" class="gc-empty">无已暂存改动</div>
        </div>
      </div>

      <div class="file-section">
        <div class="fs-head">
          <span>未暂存 ({{ unstagedFiles.length + untrackedFiles.length }})</span>
          <span class="v-spacer" />
          <button
            class="ghost"
            :disabled="busy || unstagedFiles.length + untrackedFiles.length === 0"
            @click="stageAll"
          >
            全部暂存
          </button>
        </div>
        <div class="fs-body">
          <template v-for="row in unstagedRows" :key="'w' + row.path">
            <div
              v-if="row.isDir"
              class="tree-dir"
              :style="{ paddingLeft: 8 + row.indent * 14 + 'px' }"
              @click="collapsedUnstaged = toggleCollapse(collapsedUnstaged, row.path)"
            >
              {{ collapsedUnstaged.has(row.path) ? '▸' : '▾' }} {{ row.name }}
            </div>
            <div
              v-else
              class="file-row"
              :class="{ active: selPath === row.path && !selStaged }"
              :style="{ paddingLeft: 8 + row.indent * 14 + 'px' }"
              @click="openFile(row.path, false)"
            >
              <input
                type="checkbox"
                :key="'ucb' + row.path + rev"
                :disabled="busy"
                @click.stop="action(() => stage(projectId, [row.path]))"
              />
              <span class="file-status mono">{{ fileMap.get(row.path) ? statusLetter(fileMap.get(row.path)!) : '?' }}</span>
              <span class="file-name mono">{{ row.name }}</span>
              <button
                v-if="fileMap.get(row.path) && fileMap.get(row.path)!.index !== '?'"
                class="ghost file-discard"
                :disabled="busy"
                @click.stop="discardFile(row.path)"
              >
                丢弃
              </button>
            </div>
          </template>
          <div v-if="unstagedFiles.length + untrackedFiles.length === 0" class="gc-empty">工作树干净</div>
        </div>
      </div>
    </div>

    <div class="git-col git-diff">
      <div class="gc-head">
        差异<span v-if="selPath" class="mono"> · {{ selPath }}</span><span v-if="error" class="gp-error"> · {{ error }}</span>
      </div>
      <GitDiff
        :file="diff"
        :loading="loading"
        :ignore-whitespace="ignoreWhitespace"
        :full-context="fullContext"
        @toggle-whitespace="toggleWhitespace"
        @expand-all="toggleFullContext"
      />
    </div>

    <GitCommitBox :project-id="projectId" @notify="(m, k) => emit('notify', m, k)" />
  </div>
</template>

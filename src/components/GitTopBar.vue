<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { Project, RepoStatus } from '../types'
import { relTime, sortProjects } from '../utils'
import {
  branches,
  busy,
  createBranch,
  deleteBranch,
  fetchRemote,
  loadLastFetch,
  lastFetch,
  mergeBranch,
  pullRemote,
  pushRemote,
  rebaseBranch,
  refreshRepo,
  refreshStatuses,
  renameBranch,
  selectedRepoId,
  statuses,
  switchBranch,
} from '../gitStore'

const props = defineProps<{ projects: Project[] }>()
const emit = defineEmits<{ notify: [msg: string, kind?: 'ok' | 'err'] }>()

const selected = computed(() => props.projects.find((p) => p.id === selectedRepoId.value) ?? null)
const status = computed<RepoStatus | undefined>(() => (selected.value ? statuses.value[selected.value.id] : undefined))
const list = computed(() => branches.value[selectedRepoId.value] ?? [])
const locals = computed(() => list.value.filter((b) => !b.remote))
const remotes = computed(() => list.value.filter((b) => b.remote))
const currentBranch = computed(() => list.value.find((b) => b.current)?.name ?? status.value?.branch ?? '—')
const operation = computed(() => status.value?.operation ?? '')
const blocked = computed(() => !!operation.value || busy.value)

const ahead = computed(() => status.value?.ahead ?? 0)
const behind = computed(() => status.value?.behind ?? 0)
const hasUpstream = computed(() => !!locals.value.find((b) => b.current)?.upstream)
const canPush = computed(() => !blocked.value && (ahead.value > 0 || !hasUpstream.value))

const lastFetchText = computed(() => {
  const t = selected.value ? lastFetch.value[selected.value.id] : null
  if (!t) return '从未'
  // 后端给的是 Unix 秒
  return relTime(t * 1000)
})

// ---- 仓库选择器（自定义：名称 + 路径 + 最近使用排序 + 搜索）----
const repoOpen = ref(false)
const repoQuery = ref('')
const repoSearch = ref<HTMLInputElement>()

// 这里的语义是"最近用过的排前面"，与收藏无关 → 用同一份排序实现、关掉收藏优先。
const sortedProjects = computed(() => sortProjects(props.projects, false))
const filteredProjects = computed(() => {
  const q = repoQuery.value.trim().toLowerCase()
  if (!q) return sortedProjects.value
  return sortedProjects.value.filter(
    (p) => (p.name || '').toLowerCase().includes(q) || p.rootDir.toLowerCase().includes(q),
  )
})

watch(repoOpen, async (open) => {
  if (open) {
    await nextTick()
    repoSearch.value?.focus()
  }
})

function pickRepo(id: string) {
  selectedRepoId.value = id
  repoOpen.value = false
  repoQuery.value = ''
  void refreshRepo(id)
}

function closeRepo() {
  repoOpen.value = false
}

onMounted(() => document.addEventListener('click', closeRepo))
onUnmounted(() => document.removeEventListener('click', closeRepo))

function dirty(p: Project) {
  const s = statuses.value[p.id]
  return s ? s.staged + s.unstaged + s.untracked : 0
}

/** 下拉里每个仓库的脏计数：预计算一次，别在模板里每行算 3 遍。 */
const dirtyCounts = computed<Record<string, number>>(() => {
  const out: Record<string, number> = {}
  for (const p of props.projects) out[p.id] = dirty(p)
  return out
})

const mode = ref<'' | 'new' | 'rename' | 'delete' | 'merge' | 'rebase'>('')
const input = ref('')
const checkout = ref(true)
const force = ref(false)

onMounted(() => {
  if (selectedRepoId.value) void loadLastFetch(selectedRepoId.value)
})

watch(selectedRepoId, (id) => {
  if (id) void loadLastFetch(id)
})

async function confirmDirty() {
  if (!selected.value) return true
  if (dirty(selected.value) === 0) return true
  const { confirm } = await import('@tauri-apps/plugin-dialog')
  return confirm('工作区有未提交改动，切换/合并/变基可能失败或改变文件。继续？', {
    title: '确认操作',
    kind: 'warning',
  })
}

async function onBranchChange(e: Event) {
  const select = e.target as HTMLSelectElement
  const value = select.value
  // select 绑定的是 :value，值没变时 Vue 不会把 DOM 恢复回去：
  // 取消或失败时必须手动复位，否则界面显示的分支和实际分支不一致。
  const restore = () => {
    select.value = currentBranch.value
  }
  if (!selected.value || blocked.value) return restore()
  if (value.startsWith('remote:')) {
    const remoteName = value.slice('remote:'.length)
    const short = remoteName.split('/').slice(1).join('/')
    const { confirm } = await import('@tauri-apps/plugin-dialog')
    const ok = await confirm(`基于 ${remoteName} 新建本地分支 ${short} 并切换？`, { title: '检出远程分支' })
    if (!ok) return restore()
    try {
      await createBranch(selected.value.id, short, true)
      emit('notify', `已切换到 ${short}`)
    } catch (err) {
      emit('notify', `${err}`, 'err')
    }
    restore()
    return
  }
  if (value === currentBranch.value) return
  if (!(await confirmDirty())) return restore()
  try {
    await switchBranch(selected.value.id, value)
    emit('notify', `已切换到 ${value}`)
  } catch (err) {
    emit('notify', `${err}`, 'err')
  }
  restore()
}

async function submitAction() {
  if (!selected.value) return
  const id = selected.value.id
  const name = input.value.trim()
  try {
    if (mode.value === 'new') {
      if (!name) return emit('notify', '请输入分支名', 'err')
      await createBranch(id, name, checkout.value)
      emit('notify', `已新建分支 ${name}`)
    } else if (mode.value === 'rename') {
      if (!name) return emit('notify', '请输入新分支名', 'err')
      await renameBranch(id, currentBranch.value, name)
      emit('notify', '分支已重命名')
    } else if (mode.value === 'delete') {
      if (!name) return emit('notify', '请输入要删除的分支名', 'err')
      const { confirm } = await import('@tauri-apps/plugin-dialog')
      const ok = await confirm(`删除分支 ${name}？${force.value ? '（强制，未合并提交会丢失）' : ''}`, {
        title: '删除分支',
        kind: 'warning',
      })
      if (!ok) return
      await deleteBranch(id, name, force.value)
      emit('notify', `已删除分支 ${name}`)
    } else if (mode.value === 'merge' || mode.value === 'rebase') {
      if (!name) return emit('notify', '请输入分支名', 'err')
      if (!(await confirmDirty())) return
      if (mode.value === 'merge') {
        await mergeBranch(id, name)
        emit('notify', `已合并 ${name}`)
      } else {
        await rebaseBranch(id, name)
        emit('notify', `已变基到 ${name}`)
      }
    }
    mode.value = ''
    input.value = ''
    force.value = false
  } catch (err) {
    emit('notify', `${err}`, 'err')
  }
}

function open(m: typeof mode.value) {
  if (blocked.value) return
  mode.value = mode.value === m ? '' : m
  input.value = m === 'rename' ? currentBranch.value : ''
  force.value = false
}

async function doFetch() {
  if (!selected.value) return
  try {
    await fetchRemote(selected.value.id)
    await loadLastFetch(selected.value.id)
    emit('notify', '已 fetch origin')
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function doPull() {
  if (!selected.value || blocked.value) return
  const { confirm } = await import('@tauri-apps/plugin-dialog')
  if (!(await confirm('Pull 会拉取并可能合并远程改动，继续？', { title: 'Pull', kind: 'warning' }))) return
  try {
    await pullRemote(selected.value.id)
    emit('notify', '已 pull')
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function doPush() {
  if (!selected.value || !canPush.value) return
  try {
    const r = await pushRemote(selected.value.id)
    emit('notify', r.setUpstream ? `已推送并设置 upstream（${r.branch}）` : '已推送')
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function refreshAll() {
  // 手动刷新要绕过 TTL 缓存（用户点 ⟳ 就是想要最新的）。
  await refreshStatuses(props.projects.map((p) => p.id), true)
  if (selectedRepoId.value) {
    await refreshRepo(selectedRepoId.value)
    await loadLastFetch(selectedRepoId.value)
  }
}
</script>

<template>
  <div class="git-topbar">
    <div class="gtb-field gtb-repo" @click.stop>
      <span class="gtb-label">仓库</span>
      <button class="gtb-repo-btn" :title="selected?.rootDir || ''" @click="repoOpen = !repoOpen">
        <span class="gtb-repo-name">{{ selected ? (selected.name || '未命名项目') : '选择仓库' }}</span>
        <span class="gb-caret">▾</span>
      </button>
      <div v-if="repoOpen" class="repo-pop">
        <input ref="repoSearch" v-model="repoQuery" class="mono repo-search" placeholder="搜索名称 / 路径…" spellcheck="false" />
        <div class="repo-list">
          <div
            v-for="p in filteredProjects"
            :key="p.id"
            class="repo-item"
            :class="{ active: p.id === selectedRepoId }"
            @click="pickRepo(p.id)"
          >
            <div class="repo-item-head">
              <span class="repo-item-name">{{ p.name || '未命名项目' }}</span>
              <span v-if="dirtyCounts[p.id]" class="gb-dirty mono">●{{ dirtyCounts[p.id] }}</span>
            </div>
            <div class="repo-item-path mono">{{ p.rootDir || '未设置根目录' }}</div>
          </div>
          <div v-if="filteredProjects.length === 0" class="gc-empty">无匹配仓库</div>
        </div>
      </div>
    </div>

    <label class="gtb-field">
      <span class="gtb-label">分支</span>
      <select :value="currentBranch" :disabled="blocked" @change="onBranchChange">
        <option v-for="b in locals" :key="b.name" :value="b.name">
          {{ b.name }}<template v-if="b.ahead"> ↑{{ b.ahead }}</template><template v-if="b.behind"> ↓{{ b.behind }}</template>
        </option>
        <optgroup v-if="remotes.length" label="远程">
          <option v-for="b in remotes" :key="b.name" :value="'remote:' + b.name">{{ b.name }}</option>
        </optgroup>
      </select>
    </label>

    <span class="gtb-actions">
      <button class="ghost" :disabled="blocked" title="新建分支" @click="open('new')">+ 分支</button>
      <button class="ghost" :disabled="blocked" title="合并到当前分支" @click="open('merge')">合并</button>
      <button class="ghost" :disabled="blocked" title="把当前分支变基到…" @click="open('rebase')">变基</button>
      <button class="ghost" :disabled="blocked" title="重命名当前分支" @click="open('rename')">重命名</button>
      <button class="ghost danger" :disabled="blocked" title="删除分支" @click="open('delete')">删除</button>
    </span>

    <span class="v-spacer" />
    <span class="gtb-actions">
      <button class="ghost" :disabled="busy" title="Fetch origin" @click="doFetch">Fetch</button>
      <button class="ghost" :disabled="blocked || behind === 0" :title="`Pull ${behind} 个提交`" @click="doPull">
        ↓ Pull<template v-if="behind"> {{ behind }}</template>
      </button>
      <button class="ghost" :disabled="!canPush" title="Push 到远程" @click="doPush">
        ↑ Push<template v-if="ahead"> {{ ahead }}</template>
      </button>
    </span>
    <span class="gtb-last mono">上次拉取 {{ lastFetchText }}</span>
    <span v-if="busy" class="commit-busy">处理中…</span>
    <button class="ghost" title="刷新（Ctrl+R）" @click="refreshAll">⟳</button>
  </div>

  <div v-if="mode" class="branch-form">
    <span class="mono gtb-label">
      {{ { new: '新建分支', rename: '重命名当前分支', delete: '删除分支', merge: '合并到当前', rebase: '变基当前到' }[mode] }}
    </span>
    <input v-model="input" class="inline mono" placeholder="分支名" spellcheck="false" @keydown.enter.prevent="submitAction" />
    <label v-if="mode === 'new'" class="commit-amend"><input type="checkbox" v-model="checkout" /> 并切换</label>
    <label v-if="mode === 'delete'" class="commit-amend"><input type="checkbox" v-model="force" /> 强制 (-D)</label>
    <button class="primary" @click="submitAction">确定</button>
    <button class="ghost" @click="mode = ''">取消</button>
  </div>

  <div v-if="operation" class="git-op-banner">
    存在未完成的 {{ operation }}，请在终端处理后再进行提交 / 分支操作。
  </div>
</template>

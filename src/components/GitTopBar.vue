<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Project, RepoStatus } from '../types'
import {
  branches,
  busy,
  createBranch,
  deleteBranch,
  mergeBranch,
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

const mode = ref<'' | 'new' | 'rename' | 'delete' | 'merge' | 'rebase'>('')
const input = ref('')
const checkout = ref(true)
const force = ref(false)

function dirty(p: Project) {
  const s = statuses.value[p.id]
  return s ? s.staged + s.unstaged + s.untracked : 0
}

function onRepoChange(e: Event) {
  selectedRepoId.value = (e.target as HTMLSelectElement).value
  void refreshRepo(selectedRepoId.value)
}

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
  const value = (e.target as HTMLSelectElement).value
  if (!selected.value || blocked.value) return
  // 还原选择（实际由 store 刷新后回填）
  if (value.startsWith('remote:')) {
    const remoteName = value.slice('remote:'.length)
    const short = remoteName.split('/').slice(1).join('/')
    const { confirm } = await import('@tauri-apps/plugin-dialog')
    const ok = await confirm(`基于 ${remoteName} 新建本地分支 ${short} 并切换？`, { title: '检出远程分支' })
    if (!ok) return
    try {
      await createBranch(selected.value.id, short, true)
      emit('notify', `已切换到 ${short}`)
    } catch (err) {
      emit('notify', `${err}`, 'err')
    }
    return
  }
  if (value === currentBranch.value) return
  if (!(await confirmDirty())) return
  try {
    await switchBranch(selected.value.id, value)
    emit('notify', `已切换到 ${value}`)
  } catch (err) {
    emit('notify', `${err}`, 'err')
  }
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

async function refreshAll() {
  await refreshStatuses(props.projects.map((p) => p.id))
  if (selectedRepoId.value) await refreshRepo(selectedRepoId.value)
}
</script>

<template>
  <div class="git-topbar">
    <label class="gtb-field">
      <span class="gtb-label">仓库</span>
      <select :value="selectedRepoId" @change="onRepoChange">
        <option v-for="p in projects" :key="p.id" :value="p.id">
          {{ p.name || '未命名项目' }}{{ dirty(p) ? ` ●${dirty(p)}` : '' }}
        </option>
      </select>
    </label>

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
      <button class="ghost danger" title="删除分支" @click="open('delete')">删除</button>
    </span>

    <span class="v-spacer" />
    <span v-if="busy" class="commit-busy">处理中…</span>
    <button class="bordered" @click="refreshAll">刷新</button>
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

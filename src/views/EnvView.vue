<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { config, load, persist } from '../store'
import {
  gitWorktreeAdd,
  gitWorktreePrune,
  gitWorktreeRemove,
  gitWorktreeStatuses,
  gitWorktrees,
  launchWorktree,
  openDir,
  worktreeSettings,
} from '../api'
import type { RepoStatus, WorktreeInfo, WorktreeSettings } from '../types'

const emit = defineEmits<{ (e: 'notify', msg: string, kind?: 'ok' | 'err'): void }>()
const props = defineProps<{ projectId?: string }>()

const projects = computed(() => config.value?.projects ?? [])
const selectedId = ref('')
const selected = computed(() => projects.value.find((p) => p.id === selectedId.value) ?? null)

const view = ref<{ enabled: boolean; settings: WorktreeSettings; defaultRoot: string } | null>(null)
const worktrees = ref<WorktreeInfo[]>([])
// 每个 worktree 的脏状态，按后端返回的路径原样作键（两边同源，不做前端归一化）
const statuses = ref<Record<string, RepoStatus>>({})
const busy = ref(false)
let statusSeq = 0

// 设置草稿（保存后写回配置）
const draft = reactive<{ root: string; portBase: string; portKey: string; copy: string }>({
  root: '',
  portBase: '',
  portKey: '',
  copy: '',
})

// 新建环境表单
const create = reactive({ branch: '', base: '' })

function fillDraft(s: WorktreeSettings | undefined) {
  draft.root = s?.root ?? ''
  draft.portBase = s?.portBase != null ? String(s.portBase) : ''
  draft.portKey = s?.portKey ?? ''
  draft.copy = (s?.copy ?? []).join('\n')
}

async function refresh(id: string) {
  if (!id) {
    view.value = null
    worktrees.value = []
    statuses.value = {}
    return
  }
  try {
    const [v, list] = await Promise.all([worktreeSettings(id), gitWorktrees(id)])
    view.value = v
    worktrees.value = list
    fillDraft(v.settings)
    void refreshStatuses(id)
  } catch (e) {
    emit('notify', `${e}`, 'err')
    view.value = null
    worktrees.value = []
    statuses.value = {}
  }
}

// 脏标记是附加信息：失败只退回「—」，不影响环境列表。
async function refreshStatuses(id: string) {
  const mine = ++statusSeq
  try {
    const got = await gitWorktreeStatuses(id)
    if (mine !== statusSeq) return
    const next: Record<string, RepoStatus> = {}
    for (const w of got) next[w.path] = w.status
    statuses.value = next
  } catch {
    if (mine === statusSeq) statuses.value = {}
  }
}

watch(selectedId, (id) => void refresh(id))
watch(
  () => props.projectId,
  (id) => {
    if (id) selectedId.value = id
  },
)

onMounted(() => {
  const first =
    (props.projectId && projects.value.some((p) => p.id === props.projectId) ? props.projectId : '') ||
    projects.value[0]?.id ||
    ''
  // selectedId 初值为 ''，这里赋值即触发 watch → refresh
  selectedId.value = first
})

const mainWt = computed(() => worktrees.value.find((w) => w.isMain) ?? null)
const envs = computed(() => worktrees.value.filter((w) => !w.isMain))
const hasPrunable = computed(() => worktrees.value.some((w) => w.isPrunable))

function portOf(branch: string | null): number | null {
  if (!branch) return null
  return view.value?.settings.leases.find((l) => l.branch === branch)?.port ?? null
}

interface EnvBadge {
  text: string
  cls: string
  title: string
}

// 每个环境一行脏状态徽章：加载中不显示，取不到/不可用显示「—」，冲突与未完成操作优先于计数
function badgeOf(w: WorktreeInfo): EnvBadge | null {
  if (w.isPrunable) return null
  const s = statuses.value[w.path]
  if (!s) return { text: '…', cls: 'env-mute', title: '状态读取中' }
  if (s.error || !s.isRepo) return { text: '—', cls: 'env-mute', title: s.error || '不是 git 仓库' }
  if (s.conflicts) return { text: `⚠ 冲突 ${s.conflicts}`, cls: 'env-bad', title: '有冲突文件未解决' }
  if (s.operation) return { text: s.operation, cls: 'env-bad', title: `进行中的操作：${s.operation}` }
  const n = s.staged + s.unstaged + s.untracked
  if (!n) return { text: '干净', cls: 'env-mute', title: '工作树干净' }
  return {
    text: `●${n}`,
    cls: 'env-dirty',
    title: `已暂存 ${s.staged} · 未暂存 ${s.unstaged} · 未跟踪 ${s.untracked}`,
  }
}

async function saveSettings() {
  const project = selected.value
  if (!project) return
  const portBaseText = draft.portBase.trim()
  const portBase = portBaseText ? Number(portBaseText) : null
  if (portBase != null && (!Number.isInteger(portBase) || portBase < 1 || portBase > 65_535)) {
    emit('notify', '端口起点必须是 1–65535 的整数', 'err')
    return
  }
  const settings: WorktreeSettings = {
    root: draft.root.trim() || null,
    copy: draft.copy
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean),
    portBase,
    portKey: draft.portKey.trim() || null,
    leases: project.worktree?.leases ?? view.value?.settings.leases ?? [],
  }
  // 保留既有租约，只改政策；写回 store 后持久化
  const previous = project.worktree ?? null
  project.worktree = settings
  try {
    await persist()
    await load()
    emit('notify', '环境设置已保存')
    await refresh(project.id)
  } catch (e) {
    project.worktree = previous
    emit('notify', `${e}`, 'err')
  }
}

async function doAdd() {
  const id = selectedId.value
  const branch = create.branch.trim()
  if (!id || !branch) {
    emit('notify', '请填写分支名', 'err')
    return
  }
  if (busy.value) return
  busy.value = true
  try {
    const out = await gitWorktreeAdd(id, branch, create.base.trim() || undefined)
    const bits = [`已创建环境 ${out.branch}`]
    if (out.port != null) bits.push(`端口 ${out.port}`)
    if (out.copied) bits.push(`复制 ${out.copied} 个文件`)
    if (out.skipped.length) bits.push(`跳过 ${out.skipped.length} 个已存在`)
    emit('notify', bits.join(' · '))
    create.branch = ''
    create.base = ''
    await load()
    await refresh(id)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  } finally {
    busy.value = false
  }
}

async function doLaunch(w: WorktreeInfo) {
  if (!w.branch) return
  try {
    await launchWorktree(selectedId.value, w.branch)
    await load()
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

async function doRemove(w: WorktreeInfo) {
  if (!w.branch) return
  const { confirm } = await import('@tauri-apps/plugin-dialog')
  const ok = await confirm(`删除环境 ${w.branch}？\n目录 ${w.path}`, {
    title: '删除环境',
    kind: 'warning',
  })
  if (!ok) return
  try {
    await gitWorktreeRemove(selectedId.value, w.branch, false)
    emit('notify', `已删除环境 ${w.branch}`)
    await load()
    await refresh(selectedId.value)
  } catch (e) {
    const msg = `${e}`
    if (msg.includes('强制删除')) {
      const force = await confirm(`${msg}\n\n确定强制删除（丢弃其中改动）？`, {
        title: '强制删除',
        kind: 'warning',
      })
      if (!force) return
      try {
        await gitWorktreeRemove(selectedId.value, w.branch, true)
        emit('notify', `已强制删除环境 ${w.branch}`)
        await load()
        await refresh(selectedId.value)
      } catch (e2) {
        emit('notify', `${e2}`, 'err')
      }
    } else {
      emit('notify', msg, 'err')
    }
  }
}

async function doPrune() {
  const { confirm } = await import('@tauri-apps/plugin-dialog')
  if (
    !(await confirm('清理失效记录只移除「目录已被手动删除」的环境登记，不碰任何存在的工作树。继续？', {
      title: '清理失效记录',
    }))
  )
    return
  try {
    await gitWorktreePrune(selectedId.value)
    emit('notify', '已清理失效记录')
    await refresh(selectedId.value)
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}

function openPath(path: string) {
  void openDir(path).catch((e) => emit('notify', `${e}`, 'err'))
}

function shortHead(w: WorktreeInfo): string {
  return w.head.slice(0, 7)
}

const isGitRepo = computed(() => !!selected.value && view.value !== null)
</script>

<template>
  <div class="env-page">
    <div class="page-head">
      <h1>环境</h1>
      <select v-model="selectedId" class="mono env-repo">
        <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
      <span class="page-meta">{{ envs.length }} ENVS</span>
      <span v-if="view && !view.enabled" class="badge env-off">未启用</span>
      <span class="toolbar">
        <button v-if="hasPrunable" class="ghost" @click="doPrune">清理失效记录</button>
        <button class="ghost" title="刷新" @click="refresh(selectedId)">⟳</button>
      </span>
    </div>

    <div v-if="projects.length === 0" class="empty-state">
      <div class="empty-title">还没有项目</div>
      <div class="empty-sub">先在「项目」页添加项目，再为它开启按任务环境</div>
    </div>

    <div v-else-if="!isGitRepo && selected" class="empty-state">
      <div class="empty-title">{{ selected.name }}</div>
      <div class="empty-sub">读取环境信息失败，可能不是 git 仓库或未找到 git.exe</div>
    </div>

    <template v-else-if="selected && view">
      <!-- 1 · 当前环境 -->
      <section class="section">
        <div class="section-title">当前环境<span class="grow"></span></div>
        <div class="list">
          <div v-if="mainWt" class="env-row env-row-main">
            <span class="badge">主工作区</span>
            <span class="env-branch mono">{{ mainWt.branch || (mainWt.isDetached ? '分离 HEAD' : '') }}</span>
            <span class="env-path mono" :title="mainWt.path">{{ mainWt.path }}</span>
            <span v-if="badgeOf(mainWt)" class="badge env-badge" :class="badgeOf(mainWt)!.cls" :title="badgeOf(mainWt)!.title">
              {{ badgeOf(mainWt)!.text }}
            </span>
            <span class="grow"></span>
            <button class="ghost" @click="openPath(mainWt.path)">打开目录</button>
          </div>

          <div v-for="w in envs" :key="w.path" class="env-row" :class="{ 'env-dead': w.isPrunable }">
            <span class="env-branch mono" :title="w.branch || ''">
              {{ w.branch || (w.isDetached ? '分离 HEAD' : '无分支') }}
            </span>
            <span class="env-path mono" :title="w.path">{{ w.path }}</span>
            <span v-if="portOf(w.branch) != null" class="env-port mono">:{{ portOf(w.branch) }}</span>
            <span v-if="w.isPrunable" class="badge env-bad">失效</span>
            <span v-if="badgeOf(w)" class="badge env-badge" :class="badgeOf(w)!.cls" :title="badgeOf(w)!.title">
              {{ badgeOf(w)!.text }}
            </span>
            <span class="env-sha mono">{{ shortHead(w) }}</span>
            <span class="grow"></span>
            <template v-if="!w.isPrunable && w.branch">
              <button class="primary" @click="doLaunch(w)">打开终端</button>
              <button class="ghost" @click="openPath(w.path)">目录</button>
              <button class="danger ghost" @click="doRemove(w)">删除</button>
            </template>
          </div>

          <div v-if="envs.length === 0" class="gc-empty">还没有环境，用下面的表单创建第一个。</div>
        </div>
      </section>

      <!-- 2 · 新建环境 -->
      <section class="section">
        <div class="section-title">新建环境</div>
        <div class="env-form">
          <label class="field">
            <span>分支名</span>
            <input v-model="create.branch" class="mono" placeholder="feature/…" spellcheck="false" />
          </label>
          <label class="field">
            <span>base（可选）</span>
            <input v-model="create.base" class="mono" placeholder="默认当前 HEAD" spellcheck="false" />
          </label>
          <div class="field">
            <span>&nbsp;</span>
            <button class="primary" :disabled="busy || !create.branch.trim()" @click="doAdd">创建环境</button>
          </div>
        </div>
        <div class="hint">
          创建 = <span class="mono">git worktree add</span>，随后按白名单复制本地文件并分配端口；相对目录的启动项会重解析到该环境，绝对目录项不随之漂移。
        </div>
      </section>

      <!-- 3 · 环境配置：表单不横向铺满，字段走 grid，白名单独立大输入区 -->
      <section class="section">
        <div class="section-title">环境配置</div>
        <div class="env-config">
          <div class="env-form">
            <label class="field">
              <span>环境根目录</span>
              <input v-model="draft.root" class="mono" placeholder="同级 <repo>-wt" spellcheck="false" />
            </label>
            <label class="field">
              <span>端口起点</span>
              <input v-model="draft.portBase" class="mono" placeholder="留空 = 不注入" spellcheck="false" />
            </label>
            <label class="field">
              <span>端口变量名</span>
              <input v-model="draft.portKey" class="mono" placeholder="PORT" spellcheck="false" />
            </label>
          </div>
          <div class="hint">未配置根目录时使用 {{ view.defaultRoot }}</div>

          <label class="field">
            <span>复制白名单（allow-list，每行一条；默认不复制任何本地文件）</span>
            <textarea
              v-model="draft.copy"
              class="mono"
              rows="4"
              placeholder=".env&#10;config/local.*&#10;seeds/**"
              spellcheck="false"
            ></textarea>
          </label>

          <div class="config-foot">
            <button class="primary" @click="saveSettings">保存设置</button>
            <button class="ghost" @click="fillDraft(view.settings)">还原</button>
          </div>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.env-page {
  width: 100%;
  max-width: var(--content-max);
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
}

.env-repo { min-width: 180px; }
.env-off { color: var(--faint); border-color: var(--border-strong); }
.env-bad { color: var(--danger); border-color: var(--danger); background: var(--danger-dim); }
/* 与首页 / Git 页的脏标记同一套颜色语言：琥珀=有改动，弱化=干净或不可读 */
.env-badge { flex-shrink: 0; }
.env-dirty { color: #e0b341; border-color: rgba(224, 179, 65, 0.4); }
.env-mute { color: var(--muted); }

.env-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: 8px var(--sp-4);
  font-size: 12.5px;
}

.env-row-main { background: var(--panel); }
.env-dead { opacity: 0.5; }

.env-branch {
  color: var(--signal);
  flex-shrink: 0;
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.env-path {
  color: var(--faint);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.env-port,
.env-sha {
  color: var(--muted);
  flex-shrink: 0;
}

.env-form {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: var(--sp-3);
  align-items: end;
  max-width: 900px;
}

/* 提交按钮保持自身宽度，不被 grid 拉成整列 */
.field button { align-self: flex-start; }

.env-config {
  width: 100%;
  max-width: 900px;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
  min-width: 0;
}

.field > span {
  color: var(--faint);
  font-size: 10.5px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.field textarea {
  font-size: 12px;
  line-height: 1.6;
  resize: vertical;
}

.config-foot {
  display: flex;
  gap: var(--sp-2);
  padding-top: var(--sp-3);
  border-top: 1px solid var(--border);
}

.gc-empty { padding: 10px var(--sp-4); color: var(--faint); font-size: 12px; }
</style>

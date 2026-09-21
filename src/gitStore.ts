import { ref } from 'vue'
import type { BranchInfo, GraphRow, PushResult, RepoStatus } from './types'
import {
  gitBranches,
  gitCherryPick,
  gitCommit,
  gitCreateBranch,
  gitDeleteBranch,
  gitDiscard,
  gitFileHistory,
  gitFetch,
  gitLastFetch,
  gitLog,
  gitMerge,
  gitPull,
  gitPush,
  gitRebase,
  gitRenameBranch,
  gitReset,
  gitRevert,
  gitStage,
  gitStatuses,
  gitSwitchBranch,
  gitUnstage,
  openFile,
} from './api'

export const statuses = ref<Record<string, RepoStatus>>({})
export const branches = ref<Record<string, BranchInfo[]>>({})
export const gitError = ref('')
export const logCache = ref<Record<string, GraphRow[]>>({})
export const selectedRepoId = ref('')
export const tab = ref<'changes' | 'history'>('changes')

/**
 * 状态缓存有效期。
 *
 * 窗口获得焦点、切页、切仓库都会请求状态，而每个仓库要起 2–3 个 git 子进程
 * （`rev-parse --is-inside-work-tree` + `status` + `rev-parse --git-dir`）。
 * 项目多时一次刷新就是几十个进程；没有这道闸门，用户每 alt-tab 一次就抖一次。
 */
const STATUS_TTL_MS = 4000

let statusesAt = 0
let refreshing = false
/**
 * 刷新期间到达的请求**合并成集合**，而不是"只留最后一次"。
 * 后者会静默丢掉别的调用方（首页挂载 + Git 页挂载撞车时，其中一个永远拿不到数据）。
 */
let pendingIds: Set<string> | null = null

export async function refreshStatuses(projectIds: string[], force = false): Promise<void> {
  if (refreshing) {
    const next = pendingIds ?? new Set<string>()
    for (const id of projectIds) next.add(id)
    pendingIds = next
    return
  }
  // 缓存还在有效期内、且每个项目都有数据 → 直接复用，不打 git。
  const cacheHit =
    !force &&
    Date.now() - statusesAt < STATUS_TTL_MS &&
    projectIds.every((id) => id in statuses.value)
  if (cacheHit) return

  refreshing = true
  try {
    const list = await gitStatuses(projectIds)
    // **合并**而不是替换：替换语义下，任何只传子集的调用方都会把其他仓库的状态
    // 静默清空（`statuses[p.id]` 变 undefined → 徽章消失）。合并对全量调用等价，
    // 对子集调用才是安全的。
    const next: Record<string, RepoStatus> = { ...statuses.value }
    for (const s of list) next[s.projectId] = s
    statuses.value = next
    statusesAt = Date.now()
    gitError.value = list.some((s) => s.error?.includes('未找到 git')) ? '未找到 git.exe，Git 概览不可用' : ''
  } catch (e) {
    gitError.value = `${e}`
  } finally {
    refreshing = false
    const again = pendingIds
    pendingIds = null
    if (again && again.size > 0) void refreshStatuses([...again], force)
  }
}

export async function refreshStatus(projectId: string): Promise<void> {
  try {
    const list = await gitStatuses([projectId])
    const next = { ...statuses.value }
    for (const s of list) next[s.projectId] = s
    statuses.value = next
    statusesAt = Date.now()
  } catch (e) {
    gitError.value = `${e}`
  }
}

/**
 * 每个仓库一条独立序号：并发加载不同仓库时互不干扰，同一仓库的旧响应作废。
 * 用全局单序号会让"先请求 A、再请求 B"把 A 的结果整个丢掉。
 */
const logSeq = new Map<string, number>()

async function appendLog(
  projectId: string,
  reset: boolean,
  query: string | undefined,
  author: string | undefined,
): Promise<void> {
  const seq = (logSeq.get(projectId) ?? 0) + 1
  logSeq.set(projectId, seq)
  const skip = reset ? 0 : logCache.value[projectId]?.length ?? 0
  const page = await gitLog(projectId, 100, skip, query, author)
  // 切仓库 / 重载期间到达的旧响应不能往新列表里追加，否则出现重复或空洞。
  if (logSeq.get(projectId) !== seq) return
  logCache.value = {
    ...logCache.value,
    [projectId]: reset ? page : [...(logCache.value[projectId] ?? []), ...page],
  }
}

export async function loadLog(projectId: string, reset: boolean): Promise<void> {
  await appendLog(projectId, reset, undefined, undefined)
}

export async function refreshBranches(projectId: string): Promise<void> {
  try {
    const list = await gitBranches(projectId)
    branches.value = { ...branches.value, [projectId]: list }
  } catch (e) {
    gitError.value = `${e}`
  }
}

export async function refreshRepo(projectId: string): Promise<void> {
  gitError.value = ''
  // 这里是 fire-and-forget 的调用点：任何一路失败都必须落到可见的 gitError，
  // 否则会变成一个没人处理的 rejection（用户看到的就是「点了没反应」）。
  const results = await Promise.allSettled([
    refreshStatus(projectId),
    refreshBranches(projectId),
    loadLog(projectId, true),
  ])
  const failed = results.find((r) => r.status === 'rejected')
  if (failed && failed.status === 'rejected') gitError.value = `${failed.reason}`
}

export const busy = ref(false)

/** 忙时必须报错而不是静默返回，否则用户以为点了没反应。 */
function beginBusy(): void {
  if (busy.value) throw new Error('已有 Git 操作进行中，请稍候')
  busy.value = true
}

async function runWrite(projectId: string, fn: () => Promise<unknown>): Promise<void> {
  beginBusy()
  try {
    await fn()
    await refreshRepo(projectId)
  } finally {
    busy.value = false
  }
}

export const stage = (projectId: string, paths: string[]) => runWrite(projectId, () => gitStage(projectId, paths))
export const unstage = (projectId: string, paths: string[]) => runWrite(projectId, () => gitUnstage(projectId, paths))
export const discard = (projectId: string, paths: string[]) => runWrite(projectId, () => gitDiscard(projectId, paths))
export const commit = (projectId: string, message: string, amend: boolean) =>
  runWrite(projectId, () => gitCommit(projectId, message, amend))

export const switchBranch = (projectId: string, name: string) => runWrite(projectId, () => gitSwitchBranch(projectId, name))
export const createBranch = (projectId: string, name: string, checkout: boolean) =>
  runWrite(projectId, () => gitCreateBranch(projectId, name, checkout))
export const deleteBranch = (projectId: string, name: string, force: boolean) =>
  runWrite(projectId, () => gitDeleteBranch(projectId, name, force))
export const renameBranch = (projectId: string, oldName: string, newName: string) =>
  runWrite(projectId, () => gitRenameBranch(projectId, oldName, newName))
export const mergeBranch = (projectId: string, name: string) => runWrite(projectId, () => gitMerge(projectId, name))
export const rebaseBranch = (projectId: string, onto: string) => runWrite(projectId, () => gitRebase(projectId, onto))
export const revertCommit = (projectId: string, hash: string) => runWrite(projectId, () => gitRevert(projectId, hash))
export const cherryPickCommit = (projectId: string, hash: string) =>
  runWrite(projectId, () => gitCherryPick(projectId, hash))
export const resetTo = (projectId: string, hash: string, mode: 'soft' | 'mixed') =>
  runWrite(projectId, () => gitReset(projectId, hash, mode))

export const openFileInApp = (projectId: string, path: string) => openFile(projectId, path)

export const lastFetch = ref<Record<string, number | null>>({})

export async function loadLastFetch(projectId: string): Promise<void> {
  try {
    lastFetch.value = { ...lastFetch.value, [projectId]: await gitLastFetch(projectId) }
  } catch {
    // 忽略：非仓库或缺 git
  }
}

export const fetchRemote = (projectId: string) => runWrite(projectId, () => gitFetch(projectId))
export const pullRemote = (projectId: string) => runWrite(projectId, () => gitPull(projectId))

export async function pushRemote(projectId: string): Promise<PushResult> {
  // 与 runWrite 同一把 busy 闸门（此前是复制了一份逻辑，容易只改一处）。
  beginBusy()
  try {
    const result = await gitPush(projectId)
    await refreshRepo(projectId)
    return result
  } finally {
    busy.value = false
  }
}

export async function loadLogFiltered(
  projectId: string,
  reset: boolean,
  query: string,
  author: string,
): Promise<void> {
  await appendLog(projectId, reset, query || undefined, author || undefined)
}

export async function loadFileHistory(projectId: string, path: string): Promise<GraphRow[]> {
  return gitFileHistory(projectId, path, 100)
}

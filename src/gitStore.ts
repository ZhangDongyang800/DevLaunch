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

let refreshing = false
// 刷新进行中又被叫到时记下最后一次请求，结束后补一次，而不是把这次丢掉。
let pendingIds: string[] | null = null

export async function refreshStatuses(projectIds: string[]): Promise<void> {
  if (refreshing) {
    pendingIds = projectIds
    return
  }
  refreshing = true
  try {
    const list = await gitStatuses(projectIds)
    const next: Record<string, RepoStatus> = {}
    for (const s of list) next[s.projectId] = s
    statuses.value = next
    gitError.value = list.some((s) => s.error?.includes('未找到 git')) ? '未找到 git.exe，Git 概览不可用' : ''
  } catch (e) {
    gitError.value = `${e}`
  } finally {
    refreshing = false
    const again = pendingIds
    pendingIds = null
    if (again) void refreshStatuses(again)
  }
}

export async function refreshStatus(projectId: string): Promise<void> {
  try {
    const list = await gitStatuses([projectId])
    const next = { ...statuses.value }
    for (const s of list) next[s.projectId] = s
    statuses.value = next
  } catch (e) {
    gitError.value = `${e}`
  }
}

export async function loadLog(projectId: string, reset: boolean): Promise<void> {
  const skip = reset ? 0 : logCache.value[projectId]?.length ?? 0
  const page = await gitLog(projectId, 100, skip)
  logCache.value = {
    ...logCache.value,
    [projectId]: reset ? page : [...(logCache.value[projectId] ?? []), ...page],
  }
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

async function runWrite(projectId: string, fn: () => Promise<unknown>): Promise<void> {
  // 忙时必须报错而不是静默返回，否则用户以为点了没反应。
  if (busy.value) throw new Error('已有 Git 操作进行中，请稍候')
  busy.value = true
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
  if (busy.value) throw new Error('已有 Git 操作进行中，请稍候')
  busy.value = true
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
  const skip = reset ? 0 : logCache.value[projectId]?.length ?? 0
  const page = await gitLog(projectId, 100, skip, query || undefined, author || undefined)
  logCache.value = {
    ...logCache.value,
    [projectId]: reset ? page : [...(logCache.value[projectId] ?? []), ...page],
  }
}

export async function loadFileHistory(projectId: string, path: string): Promise<GraphRow[]> {
  return gitFileHistory(projectId, path, 100)
}

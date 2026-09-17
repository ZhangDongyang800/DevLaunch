import { ref } from 'vue'
import type { BranchInfo, GraphRow, RepoStatus } from './types'
import { gitBranches, gitCommit, gitDiscard, gitLog, gitStage, gitStatuses, gitUnstage } from './api'

export const statuses = ref<Record<string, RepoStatus>>({})
export const branches = ref<Record<string, BranchInfo[]>>({})
export const gitError = ref('')
export const logCache = ref<Record<string, GraphRow[]>>({})
export const selectedRepoId = ref('')
export const tab = ref<'changes' | 'history'>('changes')

let refreshing = false

export async function refreshStatuses(projectIds: string[]): Promise<void> {
  if (refreshing) return
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
  await Promise.all([refreshStatus(projectId), refreshBranches(projectId), loadLog(projectId, true)])
}

export const busy = ref(false)

async function runWrite(projectId: string, fn: () => Promise<unknown>): Promise<void> {
  if (busy.value) return
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

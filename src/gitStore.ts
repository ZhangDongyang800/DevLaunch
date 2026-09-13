import { ref } from 'vue'
import type { GraphRow, RepoStatus } from './types'
import { gitLog, gitStatuses } from './api'

export const statuses = ref<Record<string, RepoStatus>>({})
export const gitError = ref('')
export const expandedId = ref('')
export const logCache = ref<Record<string, GraphRow[]>>({})

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

export async function loadLog(projectId: string, reset: boolean): Promise<void> {
  const skip = reset ? 0 : logCache.value[projectId]?.length ?? 0
  const page = await gitLog(projectId, 100, skip)
  logCache.value = {
    ...logCache.value,
    [projectId]: reset ? page : [...(logCache.value[projectId] ?? []), ...page],
  }
}

import type { Project } from './types'

export function filterProjects(projects: Project[], query: string): Project[] {
  const q = query.trim().toLowerCase()
  if (!q) return projects
  return projects.filter(
    (p) => p.name.toLowerCase().includes(q) || p.rootDir.toLowerCase().includes(q),
  )
}

/// 收藏置顶 → 最近启动降序 → 保持原顺序（Array.sort 稳定）。
export function sortProjects(projects: Project[]): Project[] {
  return [...projects].sort((a, b) => {
    const fav = Number(b.favorite) - Number(a.favorite)
    if (fav !== 0) return fav
    return (b.lastLaunchedAt ?? 0) - (a.lastLaunchedAt ?? 0)
  })
}

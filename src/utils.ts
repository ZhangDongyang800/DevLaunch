import type { Item, Project } from './types'

export function filterProjects(projects: Project[], query: string): Project[] {
  const q = query.trim().toLowerCase()
  if (!q) return projects
  return projects.filter(
    (p) => p.name.toLowerCase().includes(q) || p.rootDir.toLowerCase().includes(q),
  )
}

/**
 * 收藏置顶 → 最近启动降序 → 保持原顺序（Array.sort 稳定）。
 *
 * `favoritesFirst` 是给 Git 页的仓库选择器用的：那里的语义是"最近用过的排前面"，
 * 与收藏无关。此前它自己写了一份"只按 lastLaunchedAt"的排序，同一个概念两套行为
 * ——改一处不会同步另一处。
 */
export function sortProjects(projects: Project[], favoritesFirst = true): Project[] {
  return [...projects].sort((a, b) => {
    if (favoritesFirst) {
      const fav = Number(b.favorite) - Number(a.favorite)
      if (fav !== 0) return fav
    }
    return (b.lastLaunchedAt ?? 0) - (a.lastLaunchedAt ?? 0)
  })
}

/**
 * 启动项在卡片/面板里的一行标签：优先名称，否则取命令的第一个词。
 * 此前首页与搜索面板各写了一份，改一处不会同步另一处。
 *
 * 注意先 trim 再判空：`('  ' || cmd)` 里空白字符串是**真值**，直接 `||` 会得到空标签。
 */
export function itemLabel(item: Pick<Item, 'name' | 'command'>): string {
  const name = item.name.trim()
  if (name) return name
  const cmd = item.command.trim().split('\n')[0]?.trim().split(/\s+/)[0] || ''
  return cmd || '未命名'
}

/**
 * 相对时间。接受 epoch 毫秒或可被 `Date.parse` 解析的字符串。
 * 解析不出来时返回原始字符串（提交列表里宁可显示原值，也不要空着）。
 * 此前提交列表与顶栏「上次拉取」各写了一份。
 */
export function relTime(when: string | number | null | undefined): string {
  const t = typeof when === 'number' ? when : Date.parse(when ?? '')
  if (!Number.isFinite(t)) return typeof when === 'string' ? when : ''
  const d = Date.now() - t
  if (d < 60_000) return '刚刚'
  if (d < 3_600_000) return `${Math.floor(d / 60_000)} 分钟前`
  if (d < 86_400_000) return `${Math.floor(d / 3_600_000)} 小时前`
  if (d < 86_400_000 * 30) return `${Math.floor(d / 86_400_000)} 天前`
  // 超过 30 天："N 天前"已经不好读，绝对日期更有用
  return new Date(t).toLocaleDateString()
}

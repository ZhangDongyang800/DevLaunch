export interface TreeRow {
  indent: number
  name: string
  path: string
  isDir: boolean
}

interface Node {
  name: string
  path: string
  isDir: boolean
  children: Map<string, Node>
}

/// 把文件路径列表转成「扁平缩进树」；`collapsed` 里的目录不展开子节点。
export function buildTree(paths: string[], collapsed: Set<string>): TreeRow[] {
  const root: Node = { name: '', path: '', isDir: true, children: new Map() }
  for (const p of paths) {
    const parts = p.split('/')
    let cur = root
    parts.forEach((part, i) => {
      const isDir = i < parts.length - 1
      const full = parts.slice(0, i + 1).join('/')
      let next = cur.children.get(part)
      if (!next) {
        next = { name: part, path: full, isDir, children: new Map() }
        cur.children.set(part, next)
      }
      cur = next
    })
  }
  const rows: TreeRow[] = []
  const walk = (node: Node, indent: number) => {
    const sorted = [...node.children.values()].sort((a, b) =>
      a.isDir === b.isDir ? a.name.localeCompare(b.name) : a.isDir ? -1 : 1,
    )
    for (const child of sorted) {
      rows.push({ indent, name: child.name, path: child.path, isDir: child.isDir })
      if (child.isDir && !collapsed.has(child.path)) walk(child, indent + 1)
    }
  }
  walk(root, 0)
  return rows
}

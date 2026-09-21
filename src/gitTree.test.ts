import { describe, expect, it } from 'vitest'
import { buildTree } from './gitTree'

/** 把结果压成 `缩进:名字[/]` 便于断言顺序与层级。 */
function shape(paths: string[], collapsed: string[] = []): string[] {
  return buildTree(paths, new Set(collapsed)).map(
    (r) => `${r.indent}:${r.name}${r.isDir ? '/' : ''}`,
  )
}

describe('buildTree', () => {
  it('按目录分组，目录排在文件前，同级按名字排序', () => {
    expect(shape(['src/b.ts', 'src/a.ts', 'README.md'])).toEqual([
      '0:src/',
      '1:a.ts',
      '1:b.ts',
      '0:README.md',
    ])
  })

  it('折叠的目录只输出自身，不展开子节点', () => {
    expect(shape(['src/deep/a.ts', 'src/b.ts'], ['src'])).toEqual(['0:src/'])
  })

  it('多层嵌套的缩进逐级递增', () => {
    expect(shape(['a/b/c/d.txt'])).toEqual(['0:a/', '1:b/', '2:c/', '3:d.txt'])
  })

  it('同一路径重复出现只生成一行', () => {
    const rows = buildTree(['a/b', 'a/b'], new Set())
    expect(rows.filter((r) => r.path === 'a/b')).toHaveLength(1)
  })

  it('path 是完整相对路径，name 只是末段（渲染与选中的键不同）', () => {
    const [dir, file] = buildTree(['config/local.yml'], new Set())
    expect(dir).toMatchObject({ name: 'config', path: 'config', isDir: true, indent: 0 })
    expect(file).toMatchObject({ name: 'local.yml', path: 'config/local.yml', isDir: false, indent: 1 })
  })

  it('空列表返回空数组', () => {
    expect(buildTree([], new Set())).toEqual([])
  })
})

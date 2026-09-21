import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Project } from './types'
import { filterProjects, itemLabel, relTime, sortProjects } from './utils'

function project(over: Partial<Project> = {}): Project {
  return { id: 'p1', name: 'App', rootDir: 'D:\\App', favorite: false, items: [], ...over }
}

describe('filterProjects', () => {
  const list = [
    project({ id: 'a', name: 'Alpha', rootDir: 'D:\\work\\alpha' }),
    project({ id: 'b', name: 'Beta', rootDir: 'D:\\work\\beta' }),
  ]

  it('空查询原样返回（不复制）', () => {
    expect(filterProjects(list, '   ')).toBe(list)
  })

  it('按名称与路径过滤，忽略大小写', () => {
    expect(filterProjects(list, 'alph').map((p) => p.id)).toEqual(['a'])
    expect(filterProjects(list, 'D:\\WORK\\BETA').map((p) => p.id)).toEqual(['b'])
  })

  it('无匹配返回空数组', () => {
    expect(filterProjects(list, 'zzz')).toEqual([])
  })
})

describe('sortProjects', () => {
  const list = [
    project({ id: 'a', favorite: false, lastLaunchedAt: 300 }),
    project({ id: 'b', favorite: true, lastLaunchedAt: 100 }),
    project({ id: 'c', favorite: false, lastLaunchedAt: 500 }),
  ]

  it('默认收藏置顶，其次最近启动降序', () => {
    expect(sortProjects(list).map((p) => p.id)).toEqual(['b', 'c', 'a'])
  })

  it('favoritesFirst=false 时只按最近启动（Git 页仓库下拉的语义）', () => {
    expect(sortProjects(list, false).map((p) => p.id)).toEqual(['c', 'a', 'b'])
  })

  it('缺失 lastLaunchedAt 按 0 处理，排在最后', () => {
    const rows = [project({ id: 'x' }), project({ id: 'y', lastLaunchedAt: 1 })]
    expect(sortProjects(rows).map((p) => p.id)).toEqual(['y', 'x'])
  })

  it('不修改入参', () => {
    const before = list.map((p) => p.id)
    sortProjects(list)
    expect(list.map((p) => p.id)).toEqual(before)
  })
})

describe('itemLabel', () => {
  it('优先用名称', () => {
    expect(itemLabel({ name: '后端', command: 'python app.py' })).toBe('后端')
  })

  it('无名称时取命令的第一个词（只看第一行）', () => {
    expect(itemLabel({ name: '', command: 'npm run dev\nnpm test' })).toBe('npm')
  })

  it('名称只有空白也算空——空白字符串是真值，直接 || 会得到空标签', () => {
    expect(itemLabel({ name: '   ', command: 'cargo run' })).toBe('cargo')
    expect(itemLabel({ name: '', command: '' })).toBe('未命名')
    expect(itemLabel({ name: '  ', command: '   ' })).toBe('未命名')
  })
})

describe('relTime', () => {
  const now = new Date('2026-09-21T12:00:00Z').getTime()

  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(now)
  })
  afterEach(() => vi.useRealTimers())

  it('按区间给出相对时间', () => {
    expect(relTime(now - 1_000)).toBe('刚刚')
    expect(relTime(now - 5 * 60_000)).toBe('5 分钟前')
    expect(relTime(now - 3 * 3_600_000)).toBe('3 小时前')
    expect(relTime(now - 5 * 86_400_000)).toBe('5 天前')
  })

  it('边界：59 秒仍算刚刚，30 天内仍用「天前」', () => {
    expect(relTime(now - 59_000)).toBe('刚刚')
    expect(relTime(now - 60_000)).toBe('1 分钟前')
    expect(relTime(now - 29 * 86_400_000)).toBe('29 天前')
  })

  it('超过 30 天回落到绝对日期（「N 天前」已经不好读）', () => {
    const got = relTime(now - 31 * 86_400_000)
    expect(got).not.toContain('天前')
    expect(got.length).toBeGreaterThan(0)
  })

  it('接受 ISO 字符串', () => {
    expect(relTime(new Date(now - 5 * 60_000).toISOString())).toBe('5 分钟前')
  })

  it('解析不出来时返回原值，null/undefined 返回空串', () => {
    expect(relTime('不是时间')).toBe('不是时间')
    expect(relTime(null)).toBe('')
    expect(relTime(undefined)).toBe('')
  })

  it('未来时间（时钟偏差）不显示负数', () => {
    expect(relTime(now + 60_000)).toBe('刚刚')
  })
})

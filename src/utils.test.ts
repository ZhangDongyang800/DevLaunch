import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Project } from './types'
import {
  canLeaveView,
  classifyGitStatusError,
  filterProjects,
  isCurrentAsyncResult,
  isUncertainGitWriteError,
  itemLabel,
  markUncertainGitWriteError,
  moveIndex,
  positionContextMenu,
  relTime,
  sortProjects,
  waitForPending,
} from './utils'

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

describe('moveIndex', () => {
  it('在列表边界循环', () => {
    expect(moveIndex(0, -1, 3)).toBe(2)
    expect(moveIndex(2, 1, 3)).toBe(0)
  })

  it('空列表返回 0', () => {
    expect(moveIndex(4, 1, 0)).toBe(0)
  })
})

describe('canLeaveView', () => {
  it('编辑器离页保存失败时阻止导航', async () => {
    const leave = vi.fn().mockResolvedValue(false)
    await expect(canLeaveView({ name: 'editor', projectId: 'p1' }, { name: 'home' }, leave)).resolves.toBe(false)
    expect(leave).toHaveBeenCalledOnce()
  })

  it('编辑器离页保存成功后允许导航', async () => {
    const leave = vi.fn().mockResolvedValue(true)
    await expect(canLeaveView({ name: 'editor', projectId: 'p1' }, { name: 'settings' }, leave)).resolves.toBe(true)
  })

  it('非编辑页直接允许导航', async () => {
    const leave = vi.fn().mockResolvedValue(false)
    await expect(canLeaveView({ name: 'home' }, { name: 'settings' }, leave)).resolves.toBe(true)
    expect(leave).not.toHaveBeenCalled()
  })

  it('项目已不存在且编辑器未挂载时允许离开', async () => {
    await expect(canLeaveView({ name: 'editor', projectId: 'missing' }, { name: 'home' }, undefined, true)).resolves.toBe(true)
  })

  it('正常编辑器没有 leave 回调时仍阻止离开', async () => {
    await expect(canLeaveView({ name: 'editor', projectId: 'p1' }, { name: 'home' })).resolves.toBe(false)
  })
})

describe('git write uncertainty', () => {
  it('识别后端标记的超限与超时错误', () => {
    expect(isUncertainGitWriteError('git 输出超过 4 MB 上限，命令已强制终止')).toBe(true)
    expect(isUncertainGitWriteError('git 执行超时（已强制结束 Git 进程）')).toBe(true)
    expect(isUncertainGitWriteError('fatal: not a git repository')).toBe(false)
  })

  it('没有后端标记时补充不确定性文案并保留原文', () => {
    const marked = markUncertainGitWriteError('git 执行超时（已强制结束 Git 进程）')
    expect(String(marked)).toContain('git 执行超时（已强制结束 Git 进程）')
    expect(String(marked)).toContain('结果可能已部分生效')
    const ordinary = markUncertainGitWriteError('提交失败')
    expect(String(ordinary)).toBe('提交失败')
  })
})

describe('git status errors', () => {
  it('区分缺 git 与根目录不可用', () => {
    expect(classifyGitStatusError('未找到 git.exe；请在设置中指定路径').kind).toBe('git')
    expect(classifyGitStatusError('目录不存在：D:\\gone').kind).toBe('root')
    expect(classifyGitStatusError('项目未设置根目录').kind).toBe('root')
    expect(classifyGitStatusError('').kind).toBe('none')
  })
})

describe('context menu positioning', () => {
  it('靠近右下角时翻转并夹紧到视口内', () => {
    const got = positionContextMenu(
      { x: 780, y: 570 },
      { left: 700, top: 500, right: 860, bottom: 540 },
      { width: 200, height: 120 },
      { width: 800, height: 600 },
    )
    expect(got).toEqual({ x: 592, y: 380 })
  })

  it('空间足够时保留请求位置', () => {
    const got = positionContextMenu(
      { x: 120, y: 140 },
      { left: 100, top: 100, right: 300, bottom: 120 },
      { width: 200, height: 120 },
      { width: 800, height: 600 },
    )
    expect(got).toEqual({ x: 120, y: 140 })
  })
})

describe('async view guards', () => {
  it('只接受当前且仍存活的请求代次', () => {
    expect(isCurrentAsyncResult(2, 2, true)).toBe(true)
    expect(isCurrentAsyncResult(1, 2, true)).toBe(false)
    expect(isCurrentAsyncResult(2, 2, false)).toBe(false)
  })

  it('导航等待待完成的异步操作', async () => {
    let resolve!: () => void
    const pending = new Promise<void>((done) => { resolve = done })
    let settled = false
    const waiting = waitForPending(pending).then(() => { settled = true })
    await Promise.resolve()
    expect(settled).toBe(false)
    resolve()
    await waiting
    expect(settled).toBe(true)
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

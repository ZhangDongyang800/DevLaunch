export type View =
  | { name: 'home' }
  | { name: 'editor'; projectId: string }
  | { name: 'scan' }
  | { name: 'git'; projectId?: string }
  | { name: 'env'; projectId?: string }
  | { name: 'settings' }

export type Shell = 'cmd' | 'powershell' | 'bash'

export interface Item {
  id: string
  name: string
  workDir?: string | null
  shell: Shell
  command: string
}

export interface Project {
  id: string
  name: string
  rootDir: string
  favorite: boolean
  lastLaunchedAt?: number | null
  items: Item[]
  worktree?: WorktreeSettings | null
}

export interface Settings {
  autostart: boolean
  hotkey: string
  gitPath?: string | null
  theme: string
}

export interface PushResult {
  branch: string
  setUpstream: boolean
}

export interface GitInfo {
  configured: string | null
  resolved: string | null
}

export interface AppConfig {
  version: number
  settings: Settings
  projects: Project[]
}

export interface ProjectTemplate {
  version: number
  name: string
  items: Item[]
  /**
   * 模板 v4 起携带的环境**政策段**（root / copy / portBase / portKey，不含 leases）。
   * Rust 侧 `ProjectTemplate` 一直有并会序列化它——这里过去漏了，导致
   * `readProjectTemplate` 的声明类型与真实返回值不一致（类型在说谎）。
   */
  worktree?: WorktreeSettings | null
}

export interface Suggestion {
  name: string
  workDir?: string | null
  shell: Shell
  command: string
  ecosystem: string
}

export interface DetectResult {
  ecosystems: string[]
  suggestions: Suggestion[]
}

export interface DetectedProject {
  name: string
  rootDir: string
  alreadyImported: boolean
  ecosystems: string[]
  suggestions: Suggestion[]
}

export interface FileChange {
  path: string
  index: string
  worktree: string
  status: string
}

export interface RepoStatus {
  projectId: string
  isRepo: boolean
  branch: string | null
  detached: boolean
  ahead: number
  behind: number
  staged: number
  unstaged: number
  untracked: number
  conflicts: number
  operation: string | null
  /** `git status` 输出撞上 4MB 上限被截断 → 上面的计数与 files 都不完整 */
  truncated: boolean
  files: FileChange[]
  error: string | null
}

export interface BranchInfo {
  name: string
  current: boolean
  remote: boolean
  upstream: string | null
  ahead: number
  behind: number
}

export interface WorktreeLease {
  branch: string
  port: number
}

export interface WorktreeSettings {
  root?: string | null
  copy: string[]
  portBase?: number | null
  portKey?: string | null
  leases: WorktreeLease[]
}

export interface WorktreeInfo {
  path: string
  head: string
  branch: string | null
  isMain: boolean
  isDetached: boolean
  isPrunable: boolean
}

export interface WorktreeStatus {
  path: string
  branch: string | null
  status: RepoStatus
}

export interface WorktreeAddOutcome {
  path: string
  branch: string
  port: number | null
  copied: number
  skipped: string[]
}

export interface WorktreeView {
  enabled: boolean
  settings: WorktreeSettings
  defaultRoot: string
}

export interface DiffLine {
  kind: 'context' | 'add' | 'del'
  oldNo: number | null
  newNo: number | null
  text: string
}

export interface Hunk {
  header: string
  oldStart: number
  newStart: number
  lines: DiffLine[]
}

export interface FileDiff {
  path: string
  staged: boolean
  untracked: boolean
  binary: boolean
  truncated: boolean
  additions: number
  deletions: number
  hunks: Hunk[]
}

/** 二进制文件的一侧：字节数 + 魔数确认的图片 MIME + 可直接渲染的 data URL。 */
export interface BlobSide {
  size: number
  mime: string | null
  dataUrl: string | null
  /** 字节数或像素数超限（两者对用户是同一件事） */
  tooBig: boolean
  /** 从文件头读出的像素尺寸，不解码；认不出格式时为 null */
  width: number | null
  height: number | null
  /** 单独标出"解码后太大"，便于说清是哪种超限 */
  overPixels: boolean
}

export interface BinaryPreview {
  path: string
  /** 至少一侧被魔数确认为图片；否则只有字节数可看 */
  image: boolean
  old: BlobSide | null
  new: BlobSide | null
}

export interface Commit {
  hash: string
  short: string
  parents: string[]
  author: string
  email: string
  date: string
  subject: string
  refs: string[]
}

export interface GraphEdge {
  fromLane: number
  toLane: number
  parentHash: string
}

export interface GraphRow {
  commit: Commit
  lane: number
  color: number
  passes: number[]
  edges: GraphEdge[]
}

export interface CommitDetail {
  hash: string
  stat: string
  additions: number
  deletions: number
  truncated: boolean
  files: FileDiff[]
}

export type ToastKind = 'ok' | 'err'

export function newId(): string {
  return crypto.randomUUID()
}

export function newItem(): Item {
  return { id: newId(), name: '', workDir: '', shell: 'cmd', command: '' }
}

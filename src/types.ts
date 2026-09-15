export type View =
  | { name: 'home' }
  | { name: 'editor'; projectId: string }
  | { name: 'scan' }
  | { name: 'git'; projectId?: string }
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
}

export interface Settings {
  autostart: boolean
  hotkey: string
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
  files: FileChange[]
  error: string | null
}

export interface BranchInfo {
  name: string
  current: boolean
  upstream: string | null
  ahead: number
  behind: number
}

export interface FileDiff {
  path: string
  staged: boolean
  untracked: boolean
  truncated: boolean
  text: string
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
  patch: string
  truncated: boolean
}

export type ToastKind = 'ok' | 'err'

export function newId(): string {
  return crypto.randomUUID()
}

export function newItem(): Item {
  return { id: newId(), name: '', workDir: '', shell: 'cmd', command: '' }
}

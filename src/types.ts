export type View = { name: 'home' } | { name: 'editor'; projectId: string } | { name: 'settings' }

export type Shell = 'cmd' | 'powershell'

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
  items: Item[]
}

export interface Settings {
  autostart: boolean
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

export type ToastKind = 'ok' | 'err'

export function newId(): string {
  return crypto.randomUUID()
}

export function newItem(): Item {
  return { id: newId(), name: '', workDir: '', shell: 'cmd', command: '' }
}

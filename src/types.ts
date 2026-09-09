export type View = { name: 'home' } | { name: 'editor'; projectId: string } | { name: 'settings' }

export type Terminal = 'cmd' | 'powershell' | 'windowsterminal'

export type ReadyCondition =
  | { type: 'immediate' }
  | { type: 'delay'; seconds: number }
  | { type: 'port'; port: number; host: string; timeoutSec: number }
  | { type: 'process'; processName: string; timeoutSec: number }

export interface Step {
  id: string
  name: string
  workDir?: string | null
  terminal: Terminal
  command: string
  readyCondition: ReadyCondition
}

export interface Group {
  id: string
  name: string
  steps: Step[]
}

export interface Project {
  id: string
  name: string
  rootDir: string
  groups: Group[]
}

export interface Settings {
  readyTimeoutSec: number
  autostart: boolean
}

export interface AppConfig {
  version: number
  settings: Settings
  projects: Project[]
}

export type ToastKind = 'ok' | 'err'

export function newId(): string {
  return crypto.randomUUID()
}

export function newStep(): Step {
  return { id: newId(), name: '', workDir: '', terminal: 'cmd', command: '', readyCondition: { type: 'immediate' } }
}

export function newGroup(index: number): Group {
  return { id: newId(), name: `组 ${index + 1}`, steps: [] }
}

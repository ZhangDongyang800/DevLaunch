import { invoke } from '@tauri-apps/api/core'
import type { AppConfig } from './types'

export const getConfig = () => invoke<AppConfig>('get_config')
export const saveConfig = (config: AppConfig) => invoke<void>('save_config', { config })
export const launchProject = (projectId: string) => invoke<void>('launch_project_cmd', { projectId })
export const launchGroup = (projectId: string, groupId: string) =>
  invoke<void>('launch_group_cmd', { projectId, groupId })
export const runStep = (projectId: string, groupId: string, stepId: string) =>
  invoke<void>('run_step', { projectId, groupId, stepId })
export const openDir = (path: string) => invoke<void>('open_dir', { path })
export const listSubdirs = (path: string) => invoke<string[]>('list_subdirs', { path })
export const exportConfigTo = (path: string) => invoke<void>('export_config_to', { path })
export const importConfigFrom = (path: string) => invoke<void>('import_config_from', { path })
export const getAutostart = () => invoke<boolean>('get_autostart')
export const setAutostart = (enabled: boolean) => invoke<void>('set_autostart', { enabled })

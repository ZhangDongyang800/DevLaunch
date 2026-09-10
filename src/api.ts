import { invoke } from '@tauri-apps/api/core'
import type { AppConfig, ProjectTemplate } from './types'

export const getConfig = () => invoke<AppConfig>('get_config')
export const saveConfig = (config: AppConfig) => invoke<void>('save_config', { config })
export const launchProject = (projectId: string) => invoke<void>('launch_project_cmd', { projectId })
export const launchItem = (projectId: string, itemId: string) =>
  invoke<void>('launch_item_cmd', { projectId, itemId })
export const openDir = (path: string) => invoke<void>('open_dir', { path })
export const listSubdirs = (path: string) => invoke<string[]>('list_subdirs', { path })
export const exportConfigTo = (path: string) => invoke<void>('export_config_to', { path })
export const importConfigFrom = (path: string) => invoke<void>('import_config_from', { path })
export const exportProject = (projectId: string, path: string) =>
  invoke<void>('export_project', { projectId, path })
export const exportProjectFile = (projectId: string) =>
  invoke<string>('export_project_file', { projectId })
export const readProjectTemplate = (path: string) =>
  invoke<ProjectTemplate>('read_project_template', { path })
export const getAutostart = () => invoke<boolean>('get_autostart')
export const setAutostart = (enabled: boolean) => invoke<void>('set_autostart', { enabled })

import { invoke } from '@tauri-apps/api/core'
import type {
  AppConfig,
  BranchInfo,
  CommitDetail,
  DetectResult,
  DetectedProject,
  FileDiff,
  GraphRow,
  ProjectTemplate,
  RepoStatus,
} from './types'

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

export const detectProject = (path: string) => invoke<DetectResult>('detect_project', { path })
export const scanWorkspace = (path: string) => invoke<DetectedProject[]>('scan_workspace', { path })

export const setHotkey = (hotkey: string) => invoke<void>('set_hotkey', { hotkey })
export const hidePalette = () => invoke<void>('hide_palette')

export const gitStatuses = (projectIds: string[]) =>
  invoke<RepoStatus[]>('git_statuses', { projectIds })
export const gitLog = (projectId: string, limit?: number, skip?: number) =>
  invoke<GraphRow[]>('git_log', { projectId, limit, skip })
export const gitCommitDetail = (projectId: string, hash: string) =>
  invoke<CommitDetail>('git_commit_detail', { projectId, hash })
export const gitBranches = (projectId: string) => invoke<BranchInfo[]>('git_branches', { projectId })
export const gitFileDiff = (projectId: string, path: string, staged: boolean) =>
  invoke<FileDiff>('git_file_diff', { projectId, path, staged })

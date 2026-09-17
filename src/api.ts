import { invoke } from '@tauri-apps/api/core'
import type {
  AppConfig,
  BranchInfo,
  CommitDetail,
  DetectResult,
  DetectedProject,
  FileDiff,
  GraphRow,
  GitInfo,
  ProjectTemplate,
  PushResult,
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
export const gitLog = (projectId: string, limit?: number, skip?: number, query?: string, author?: string) =>
  invoke<GraphRow[]>('git_log', { projectId, limit, skip, query, author })
export const gitFileHistory = (projectId: string, path: string, limit?: number) =>
  invoke<GraphRow[]>('git_file_history', { projectId, path, limit })
export const gitCommitDetail = (projectId: string, hash: string) =>
  invoke<CommitDetail>('git_commit_detail', { projectId, hash })
export const gitBranches = (projectId: string) => invoke<BranchInfo[]>('git_branches', { projectId })
export const gitFileDiff = (
  projectId: string,
  path: string,
  staged: boolean,
  ignoreWhitespace: boolean,
  fullContext: boolean,
) => invoke<FileDiff>('git_file_diff', { projectId, path, staged, ignoreWhitespace, fullContext })
export const gitStage = (projectId: string, paths: string[]) => invoke<void>('git_stage', { projectId, paths })
export const gitUnstage = (projectId: string, paths: string[]) => invoke<void>('git_unstage', { projectId, paths })
export const gitDiscard = (projectId: string, paths: string[]) => invoke<void>('git_discard', { projectId, paths })
export const gitCommit = (projectId: string, message: string, amend: boolean) =>
  invoke<string>('git_commit', { projectId, message, amend })
export const gitSwitchBranch = (projectId: string, name: string) =>
  invoke<void>('git_switch_branch', { projectId, name })
export const gitCreateBranch = (projectId: string, name: string, checkout: boolean) =>
  invoke<void>('git_create_branch', { projectId, name, checkout })
export const gitDeleteBranch = (projectId: string, name: string, force: boolean) =>
  invoke<void>('git_delete_branch', { projectId, name, force })
export const gitRenameBranch = (projectId: string, oldName: string, newName: string) =>
  invoke<void>('git_rename_branch', { projectId, oldName, newName })
export const gitMerge = (projectId: string, name: string) => invoke<void>('git_merge', { projectId, name })
export const gitRebase = (projectId: string, onto: string) => invoke<void>('git_rebase', { projectId, onto })
export const gitRevert = (projectId: string, hash: string) => invoke<void>('git_revert', { projectId, hash })
export const gitCherryPick = (projectId: string, hash: string) => invoke<void>('git_cherry_pick', { projectId, hash })
export const gitReset = (projectId: string, hash: string, mode: 'soft' | 'mixed') =>
  invoke<void>('git_reset', { projectId, hash, mode })
export const openFile = (projectId: string, path: string) => invoke<void>('open_file', { projectId, path })
export const gitFetch = (projectId: string) => invoke<void>('git_fetch', { projectId })
export const gitPull = (projectId: string) => invoke<void>('git_pull', { projectId })
export const gitPush = (projectId: string) => invoke<PushResult>('git_push', { projectId })
export const gitLastFetch = (projectId: string) => invoke<number | null>('git_last_fetch', { projectId })
export const getGitInfo = () => invoke<GitInfo>('get_git_info')

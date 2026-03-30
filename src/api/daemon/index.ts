export { daemonClient } from './client'
export type { RequestOptions } from './client'
export type { DaemonConfig, SessionToken } from './types'
export { isSessionExpired } from './types'
export { DaemonApiError, DaemonErrorCode, mapStatusToErrorCode } from './errors'
export { getSettings, updateSettings } from './settings'
export type {
  Settings,
  GeneralSettings,
  SyncSettings,
  SecuritySettings,
  PairingSettings,
  FileSyncSettings,
  ContentTypes,
  RetentionPolicy,
  RetentionRule,
  ShortcutKey,
  Theme,
  UpdateChannel,
  SyncFrequency,
  RuleEvaluation,
} from './settings'
export { getEncryptionState, unlockEncryption, lockEncryption } from './encryption'
export type { EncryptionStateResponse } from './encryption'
export {
  getClipboardEntries,
  getClipboardEntry,
  deleteClipboardEntry,
  restoreClipboardEntry,
  toggleFavorite,
  getClipboardStats,
  getClipboardEntryResource,
} from './clipboard'
export type {
  ClipboardEntryDto,
  ClipboardEntriesResponse,
  ClipboardStats,
  ClipboardEntryResource,
  RestoreResult,
} from './clipboard'
export { getStorageStats, clearCache } from './storage'
export type { StorageStats } from './storage'

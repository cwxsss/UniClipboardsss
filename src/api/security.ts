/**
 * Security API module — typed accessors for daemon encryption endpoints and Tauri commands.
 *
 * 安全 API 模块 — daemon 加密端点和 Tauri 命令的类型化访问器。
 *
 * # Daemon Endpoints / Daemon 端点
 * - `GET /encryption/state` → current encryption initialization & session state
 * - `POST /encryption/unlock` → unlock encryption session with passphrase
 * - `POST /encryption/lock` → lock encryption session (clear master key)
 *
 * # Tauri Commands / Tauri 命令
 * The following commands require native OS integration and remain on Tauri:
 * - `get_encryption_password` → read from macOS Keychain
 * - `set_encryption_password` → write to macOS Keychain
 * - `delete_encryption_password` → delete from macOS Keychain
 * - `unlock_encryption_session` → passphrase unlock (legacy)
 * - `verify_keychain_access` → check Keychain "Always Allow" permission
 */

import { invokeWithTrace } from '@/lib/tauri-command'
import { getEncryptionState as daemonGetEncryptionState } from './daemon/encryption'

// ── Types ─────────────────────────────────────────────────────

/**
 * Encryption session status.
 *
 * 加密会话状态。
 *
 * Field names match the daemon API response (camelCase).
 */
export interface EncryptionSessionStatus {
  initialized: boolean
  sessionReady: boolean
}

// ── Daemon-based functions ─────────────────────────────────────

/**
 * Fetch encryption session status from the daemon.
 *
 * 从 daemon 获取加密会话状态。
 *
 * Uses daemon HTTP API: `GET /encryption/state`
 *
 * @returns Encryption initialization and session readiness.
 * @throws {DaemonApiError} On HTTP or session errors.
 */
export async function getEncryptionSessionStatus(): Promise<EncryptionSessionStatus> {
  return daemonGetEncryptionState()
}

// ── Tauri-based functions (require native OS integration) ───────

/**
 * Get the encryption passphrase from macOS Keychain.
 *
 * 从 macOS Keychain 获取加密密码短语。
 *
 * @returns The stored encryption passphrase.
 * @throws On Keychain errors or if no passphrase is stored.
 */
export async function getEncryptionPassword(): Promise<string> {
  try {
    return await invokeWithTrace('get_encryption_password')
  } catch (error) {
    console.error('Failed to get encryption password:', error)
    throw error
  }
}

/**
 * Store the encryption passphrase in macOS Keychain.
 *
 * 将加密密码短语存储到 macOS Keychain。
 *
 * @param password The passphrase to store.
 * @returns True on success.
 * @throws On Keychain errors.
 */
export async function setEncryptionPassword(password: string): Promise<boolean> {
  try {
    return await invokeWithTrace('set_encryption_password', { password })
  } catch (error) {
    console.error('Failed to set encryption password:', error)
    throw error
  }
}

/**
 * Delete the encryption passphrase from macOS Keychain.
 *
 * 从 macOS Keychain 删除加密密码短语。
 *
 * @returns True on success.
 * @throws On Keychain errors.
 */
export async function deleteEncryptionPassword(): Promise<boolean> {
  try {
    return await invokeWithTrace('delete_encryption_password')
  } catch (error) {
    console.error('Failed to delete encryption password:', error)
    throw error
  }
}

/**
 * Unlock the encryption session (legacy Tauri command).
 *
 * 解锁加密会话（旧的 Tauri 命令）。
 *
 * @returns True on success.
 * @throws On unlock errors.
 */
export async function unlockEncryptionSession(): Promise<boolean> {
  try {
    return await invokeWithTrace('unlock_encryption_session')
  } catch (error) {
    console.error('Failed to unlock encryption session:', error)
    throw error
  }
}

/**
 * Verify macOS Keychain "Always Allow" permission for this app.
 *
 * 验证此应用的 macOS Keychain "始终允许" 权限。
 *
 * @returns True if permission is granted.
 * @throws On permission check errors.
 */
export async function verifyKeychainAccess(): Promise<boolean> {
  try {
    return await invokeWithTrace('verify_keychain_access')
  } catch (error) {
    console.error('Keychain verification failed:', error)
    throw error
  }
}

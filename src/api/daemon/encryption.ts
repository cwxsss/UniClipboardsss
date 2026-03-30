/**
 * Encryption API module — typed accessors for daemon encryption endpoints.
 *
 * 加密 API 模块 — daemon 加密端点的类型化访问器。
 *
 * # Endpoints / 端点
 * - `GET /encryption/state` → current encryption initialization & session state
 * - `POST /encryption/unlock` → unlock encryption session with passphrase
 * - `POST /encryption/lock` → lock encryption session (clear master key)
 */

import { daemonClient } from './client'

// ── Response types ─────────────────────────────────────────────

/**
 * Encryption state as returned by `GET /encryption/state`.
 *
 * 加密状态，由 `GET /encryption/state` 返回。
 *
 * Field names are camelCase to match daemon serde `rename_all = "camelCase"`.
 */
export interface EncryptionStateResponse {
  /** Whether encryption has been initialized (passphrase configured). */
  initialized: boolean
  /** Whether the encryption session is currently unlocked and ready. */
  sessionReady: boolean
}

/** Wrapper for GET /encryption/state JSON envelope. */
interface EncryptionStateEnvelope {
  data: EncryptionStateResponse
  ts: number
}

/** Wrapper for POST /encryption/unlock and /encryption/lock JSON envelope. */
interface EncryptionActionEnvelope {
  data: { success: boolean }
  ts: number
}

// ── Public API ─────────────────────────────────────────────────

/**
 * Fetch the current encryption state from the daemon.
 *
 * 从 daemon 获取当前加密状态。
 *
 * @returns Encryption initialization and session readiness.
 * @throws {DaemonApiError} On HTTP or session errors.
 */
export async function getEncryptionState(): Promise<EncryptionStateResponse> {
  const res = await daemonClient.request<EncryptionStateEnvelope>('/encryption/state')
  return res.data
}

/**
 * Unlock the encryption session using the provided passphrase.
 *
 * 使用提供的密码短语解锁加密会话。
 *
 * @param passphrase The user's encryption passphrase.
 * @throws {DaemonApiError} On wrong passphrase (401), not initialized (400), or other errors.
 */
export async function unlockEncryption(passphrase: string): Promise<void> {
  await daemonClient.request<EncryptionActionEnvelope>('/encryption/unlock', {
    method: 'POST',
    body: { passphrase },
  })
}

/**
 * Lock the encryption session, clearing the master key from memory.
 *
 * 锁定加密会话，从内存中清除主密钥。
 *
 * @throws {DaemonApiError} On HTTP or session errors.
 */
export async function lockEncryption(): Promise<void> {
  await daemonClient.request<EncryptionActionEnvelope>('/encryption/lock', {
    method: 'POST',
  })
}

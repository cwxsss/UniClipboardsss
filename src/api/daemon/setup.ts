/**
 * Setup API module — typed accessors for daemon setup endpoints.
 *
 * Setup API 模块 — daemon 设置端点的类型化访问器。
 *
 * # Endpoints / 端点
 * - `GET /setup/state` → current setup state
 * - `POST /setup/host` → start new space (host) flow
 * - `POST /setup/join` → start join space flow
 * - `POST /setup/select-peer` → select a peer device to join (body: `{ peerId }`)
 * - `POST /setup/submit-passphrase` → submit passphrase for new space (body: `{ passphrase }`)
 * - `POST /setup/confirm-peer` → confirm trust for selected peer
 * - `POST /setup/cancel` → cancel setup flow
 */

import { daemonClient } from './client'

// ── Types (defined here to avoid circular imports with facade) ────────

export type SetupError =
  | 'PassphraseMismatch'
  | 'PassphraseEmpty'
  | { PassphraseTooShort: { min_len: number } }
  | 'PassphraseInvalidOrMismatch'
  | 'NetworkTimeout'
  | 'PeerUnavailable'
  | 'PairingRejected'
  | 'PairingFailed'

export type SetupState =
  | 'Welcome'
  | { CreateSpaceInputPassphrase: { error: SetupError | null } }
  | { JoinSpaceSelectDevice: { error: SetupError | null } }
  | {
      JoinSpaceConfirmPeer: {
        short_code: string
        peer_fingerprint?: string | null
        error: SetupError | null
      }
    }
  | { JoinSpaceInputPassphrase: { error: SetupError | null } }
  | { ProcessingCreateSpace: { message: string | null } }
  | { ProcessingJoinSpace: { message: string | null } }
  | 'Completed'

export interface SetupStateChangedEvent {
  sessionId: string
  state: SetupState
  source?: string
  ts: number
}

export interface SpaceAccessCompletedEvent {
  sessionId: string
  peerId: string
  success: boolean
  reason?: string | null
  ts: number
}

/**
 * Submit passphrase result (mirrors Tauri command contract).
 *
 * If the two passphrases do not match, returns early with PassphraseMismatch
 * without calling the daemon. Otherwise calls POST /setup/submit-passphrase.
 */
export async function submitPassphrase(
  passphrase1: string,
  passphrase2: string
): Promise<SetupState> {
  if (passphrase1 !== passphrase2) {
    return {
      CreateSpaceInputPassphrase: {
        error: 'PassphraseMismatch',
      },
    }
  }
  return daemonClient.request<SetupState>('/setup/submit-passphrase', {
    method: 'POST',
    body: { passphrase: passphrase1 },
  })
}

/**
 * Get current setup state.
 *
 * 获取当前设置流程状态。
 */
export async function getSetupState(): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/state')
}

/**
 * Start new space flow.
 *
 * 启动新空间流程。
 */
export async function startNewSpace(): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/host', { method: 'POST' })
}

/**
 * Start join space flow.
 *
 * 启动加入空间流程。
 */
export async function startJoinSpace(): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/join', { method: 'POST' })
}

/**
 * Select peer device to join.
 *
 * 选择加入空间的设备。
 */
export async function selectJoinPeer(peerId: string): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/select-peer', {
    method: 'POST',
    body: { peerId },
  })
}

/**
 * Submit passphrase for new space (verifies match already done in facade).
 *
 * 提交新空间口令。
 */
export async function submitNewSpacePassphrase(passphrase: string): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/submit-passphrase', {
    method: 'POST',
    body: { passphrase },
  })
}

/**
 * Verify passphrase for join space.
 *
 * 校验加入空间口令。
 */
export async function verifyPassphrase(passphrase: string): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/verify-passphrase', {
    method: 'POST',
    body: { passphrase },
  })
}

/**
 * Confirm trust for the selected peer device.
 *
 * 确认选中设备的可信度。
 */
export async function confirmPeerTrust(): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/confirm-peer', { method: 'POST' })
}

/**
 * Cancel setup flow.
 *
 * 取消设置流程。
 */
export async function cancelSetup(): Promise<SetupState> {
  return daemonClient.request<SetupState>('/setup/cancel', { method: 'POST' })
}

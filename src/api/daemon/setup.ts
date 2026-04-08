/**
 * Setup API module — typed accessors for daemon setup endpoints.
 *
 * Setup API 模块 — daemon 设置端点的类型化访问器。
 *
 * # Endpoints / 端点
 * - `GET /setup/state` → current setup state
 * - `POST /setup/new` → start new space (host) flow
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

export interface SetupStateResponse {
  state: SetupState
  sessionId: string | null
  nextStepHint: string | null
  profile: string | null
  clipboardMode: string | null
  deviceName: string | null
  peerId: string | null
  selectedPeerId: string | null
  selectedPeerName: string | null
  hasCompleted: boolean
}

export interface SpaceAccessCompletedEvent {
  sessionId: string
  peerId: string
  success: boolean
  reason?: string | null
  ts: number
}

type LegacySetupApiResponse = { data: SetupStateResponse; ts: number }

/** Runtime response shapes observed across setup endpoints. */
type SetupApiResponse = (SetupStateResponse & { ts: number }) | LegacySetupApiResponse

function extractSetupState(response: SetupApiResponse): SetupState {
  if ('data' in response) {
    return response.data.state
  }
  return response.state
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
  const response = await daemonClient.request<SetupApiResponse>('/setup/submit-passphrase', {
    method: 'POST',
    body: { passphrase: passphrase1 },
  })
  return extractSetupState(response)
}

/**
 * Get current setup state.
 *
 * 获取当前设置流程状态。
 *
 * The daemon API returns a flat response with metadata fields beside `state`.
 * This function extracts and returns only the current setup state.
 */
export async function getSetupState(): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/state')
  return extractSetupState(response)
}

/**
 * Start new space flow.
 *
 * 启动新空间流程。
 */
export async function startNewSpace(): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/new', {
    method: 'POST',
  })
  return extractSetupState(response)
}

/**
 * Start join space flow.
 *
 * 启动加入空间流程。
 */
export async function startJoinSpace(): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/join', {
    method: 'POST',
  })
  return extractSetupState(response)
}

/**
 * Select peer device to join.
 *
 * 选择加入空间的设备。
 */
export async function selectJoinPeer(peerId: string): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/select-peer', {
    method: 'POST',
    body: { peerId },
  })
  return extractSetupState(response)
}

/**
 * Submit passphrase for new space (verifies match already done in facade).
 *
 * 提交新空间口令。
 */
export async function submitNewSpacePassphrase(passphrase: string): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/submit-passphrase', {
    method: 'POST',
    body: { passphrase },
  })
  return extractSetupState(response)
}

/**
 * Verify passphrase for join space.
 *
 * 校验加入空间口令。
 */
export async function verifyPassphrase(passphrase: string): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/verify-passphrase', {
    method: 'POST',
    body: { passphrase },
  })
  return extractSetupState(response)
}

/**
 * Confirm trust for the selected peer device.
 *
 * 确认选中设备的可信度。
 */
export async function confirmPeerTrust(): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/confirm-peer', {
    method: 'POST',
  })
  return extractSetupState(response)
}

/**
 * Cancel setup flow.
 *
 * 取消设置流程。
 */
export async function cancelSetup(): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/cancel', {
    method: 'POST',
  })
  return extractSetupState(response)
}

/**
 * Complete space access — transitions the setup orchestrator to Completed.
 *
 * 完成空间访问 — 将设置编排器转换到 Completed 状态。
 *
 * Called when the daemon emits `setup.spaceAccessCompleted` via the WebSocket
 * bridge. For the sponsor (already Completed), the daemon returns the current
 * state without dispatching any transition.
 */
export async function completeSpaceAccess(): Promise<SetupState> {
  const response = await daemonClient.request<SetupApiResponse>('/setup/complete-space-access', {
    method: 'POST',
  })
  return extractSetupState(response)
}

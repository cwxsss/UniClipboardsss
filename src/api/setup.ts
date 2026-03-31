/**
 * Setup API — stable facade layer delegating to daemon HTTP endpoints.
 *
 * This module is the public API surface for setup functionality.
 * All forwarding calls go through daemon HTTP; only Tauri-specific
 * operations (handleSpaceAccessCompleted, event listeners) remain here.
 */

import {
  getSetupState as daemonGetSetupState,
  startNewSpace as daemonStartNewSpace,
  startJoinSpace as daemonStartJoinSpace,
  selectJoinPeer as daemonSelectJoinPeer,
  submitPassphrase as daemonSubmitPassphrase,
  verifyPassphrase as daemonVerifyPassphrase,
  confirmPeerTrust as daemonConfirmPeerTrust,
  cancelSetup as daemonCancelSetup,
} from '@/api/daemon/setup'
import type {
  SetupState,
  SetupStateChangedEvent,
  SpaceAccessCompletedEvent,
} from '@/api/daemon/setup'
import { onDaemonRealtimeEvent } from '@/api/realtime'
import { invokeWithTrace } from '@/lib/tauri-command'

// Types are defined in the daemon module to avoid circular imports.
// Re-export them so consumers can import from either location.
export type {
  SetupError,
  SetupState,
  SetupStateChangedEvent,
  SpaceAccessCompletedEvent,
} from '@/api/daemon/setup'

/**
 * Get current setup state
 * 获取当前设置流程状态
 */
export async function getSetupState(): Promise<SetupState> {
  return daemonGetSetupState()
}

/**
 * Start new space flow
 * 启动新空间流程
 */
export async function startNewSpace(): Promise<SetupState> {
  return daemonStartNewSpace()
}

/**
 * Start join space flow
 * 启动加入空间流程
 */
export async function startJoinSpace(): Promise<SetupState> {
  return daemonStartJoinSpace()
}

/**
 * Select peer device to join
 * 选择加入空间的设备
 */
export async function selectJoinPeer(peerId: string): Promise<SetupState> {
  return daemonSelectJoinPeer(peerId)
}

/**
 * Submit passphrase for new space
 * 提交新空间口令
 */
export async function submitPassphrase(
  passphrase1: string,
  passphrase2: string
): Promise<SetupState> {
  // Local mismatch check is handled inside daemonSubmitPassphrase.
  return daemonSubmitPassphrase(passphrase1, passphrase2)
}

/**
 * Verify passphrase for join space
 * 校验加入空间口令
 */
export async function verifyPassphrase(passphrase: string): Promise<SetupState> {
  return daemonVerifyPassphrase(passphrase)
}

/**
 * Confirm trust for the selected peer device
 * 确认选中设备的可信度
 */
export async function confirmPeerTrust(): Promise<SetupState> {
  return daemonConfirmPeerTrust()
}

/**
 * Cancel setup flow
 * 取消设置流程
 */
export async function cancelSetup(): Promise<SetupState> {
  return daemonCancelSetup()
}

/**
 * Called by the frontend when the daemon emits `setup.spaceAccessCompleted` via
 * the WebSocket bridge. This bridges the gap between the daemon's space access
 * orchestrator completing and the app's setup orchestrator transitioning to
 * `Completed`.
 */
export async function handleSpaceAccessCompleted(): Promise<SetupState> {
  return (await invokeWithTrace('handle_space_access_completed')) as SetupState
}

/**
 * Listen for space access completion events from the daemon.
 * This is used to transition the setup state machine from `ProcessingJoinSpace`
 * to `Completed` when the daemon's space access flow completes.
 */
export async function onSpaceAccessCompleted(
  callback: (event: SpaceAccessCompletedEvent) => void
): Promise<() => void> {
  return onDaemonRealtimeEvent(event => {
    if (event.topic !== 'setup' || event.type !== 'setup.spaceAccessCompleted') {
      return
    }
    callback(event.payload as SpaceAccessCompletedEvent)
  })
}

/**
 * Listen setup state changes with session idempotency.
 */
export async function onSetupStateChanged(
  callback: (event: SetupStateChangedEvent) => void
): Promise<() => void> {
  let activeSessionId: string | null = null
  const seenEventKeys = new Set<string>()

  return onDaemonRealtimeEvent(event => {
    if (event.topic !== 'setup' || event.type !== 'setup.stateChanged') {
      return
    }

    const payload = event.payload as Omit<SetupStateChangedEvent, 'ts' | 'source'>
    const enrichedEvent: SetupStateChangedEvent = {
      ...payload,
      source: 'realtime',
      ts: event.ts,
    }

    if (!enrichedEvent.sessionId) {
      return
    }

    if (activeSessionId !== enrichedEvent.sessionId) {
      activeSessionId = enrichedEvent.sessionId
      seenEventKeys.clear()
    }

    const dedupeKey = `${enrichedEvent.sessionId}:${JSON.stringify(enrichedEvent.state)}:${enrichedEvent.ts}`
    if (seenEventKeys.has(dedupeKey)) {
      return
    }
    seenEventKeys.add(dedupeKey)

    callback(enrichedEvent)

    if (enrichedEvent.state === 'Completed') {
      activeSessionId = null
      seenEventKeys.clear()
    }
  })
}

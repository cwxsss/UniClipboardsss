/**
 * Shared event utilities for daemon WebSocket event handling.
 *
 * Provides:
 * - diffPeerSnapshots: converts full peer snapshots into discovered/lost events
 * - classifyPairingError: maps error strings to PairingErrorKind
 */

import type { PairingErrorKind } from '@/api/daemon/pairing'

export interface PeerSnapshotPeer {
  peerId: string
  deviceName?: string | null
  connected: boolean
}

export interface PeerDiffEvent {
  peerId: string
  deviceName?: string | null
  addresses: string[]
  discovered: boolean
}

/**
 * Converts a full peer snapshot into discovered/lost diff events.
 *
 * 将完整的 peer 快照转换为 discovered/lost 差分事件。
 *
 * Mutates `knownPeers` in-place to track the current snapshot state.
 * 直接修改 `knownPeers` 以跟踪当前快照状态。
 *
 * @param nextPeers - The current full list of peers from the server.
 * @param knownPeers - Mutable map tracking previously known peers.
 * @param callback - Called once per diff event (discovered or lost).
 */
export function diffPeerSnapshots(
  nextPeers: PeerSnapshotPeer[],
  knownPeers: Map<string, { deviceName?: string | null }>,
  callback: (event: PeerDiffEvent) => void
): void {
  const nextMap = new Map<string, { deviceName?: string | null }>()
  for (const peer of nextPeers) {
    nextMap.set(peer.peerId, { deviceName: peer.deviceName ?? null })
    if (!knownPeers.has(peer.peerId)) {
      callback({
        peerId: peer.peerId,
        deviceName: peer.deviceName ?? null,
        addresses: [],
        discovered: true,
      })
    }
  }

  for (const [peerId, previous] of knownPeers.entries()) {
    if (!nextMap.has(peerId)) {
      callback({
        peerId,
        deviceName: previous.deviceName ?? null,
        addresses: [],
        discovered: false,
      })
    }
  }

  knownPeers.clear()
  for (const [peerId, peer] of nextMap.entries()) {
    knownPeers.set(peerId, peer)
  }
}

/**
 * Maps a raw error string to a typed PairingErrorKind.
 *
 * 将原始错误字符串映射到类型化的 PairingErrorKind。
 */
export function classifyPairingError(rawError?: string | null): PairingErrorKind {
  const normalized = rawError?.toLowerCase() ?? ''

  if (
    normalized.includes('active pairing session exists') ||
    normalized.includes('active_session_exists')
  ) {
    return 'active_session_exists'
  }

  if (
    normalized.includes('no local pairing participant ready') ||
    normalized.includes('no_local_participant')
  ) {
    return 'no_local_participant'
  }

  if (
    normalized.includes('pairing session not found') ||
    normalized.includes('session_not_found') ||
    normalized.includes('session expired')
  ) {
    return 'session_not_found'
  }

  if (
    normalized.includes('daemon connection info is not available') ||
    normalized.includes('connection refused') ||
    normalized.includes('failed to call daemon pairing route') ||
    normalized.includes('failed to open daemon tcp socket') ||
    normalized.includes('failed to connect daemon websocket') ||
    normalized.includes('pairing_host_unavailable')
  ) {
    return 'daemon_unavailable'
  }

  return 'unknown'
}

export type { PairingErrorKind }

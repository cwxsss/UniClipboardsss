/**
 * P2P device discovery and pairing API
 *
 * 提供 libp2p 设备发现、配对和剪贴板同步功能
 */

import { listen } from '@tauri-apps/api/event'
import { onDaemonRealtimeEvent } from '@/api/realtime'
import { invokeWithTrace } from '@/lib/tauri-command'

/**
 * P2P 设备信息
 */
export interface P2PPeerInfo {
  /** Peer ID (libp2p identifier) */
  peerId: string
  /** Device name (may be null if not yet discovered) */
  deviceName?: string | null
  /** Addresses */
  addresses: string[]
  /** Whether this peer is paired */
  isPaired: boolean
  /** Connection status */
  connected: boolean
}

/**
 * 本地设备信息
 */
export interface LocalDeviceInfo {
  /** Peer ID */
  peerId: string
  /** Device name */
  deviceName: string
}

/**
 * 已配对的设备信息
 */
export interface PairedPeer {
  /** Peer ID */
  peerId: string
  /** Device name */
  deviceName: string
  /** Shared secret (encrypted) */
  sharedSecret: number[]
  /** Pairing timestamp (ISO 8601) */
  pairedAt: string
  /** Last seen timestamp (ISO 8601) */
  lastSeen: string | null
  /** Last known addresses */
  lastKnownAddresses: string[]
  /** Connection status */
  connected: boolean
}

/**
 * P2P 配对请求
 */
export interface P2PPairingRequest {
  /** Target peer ID */
  peerId: string
}

/**
 * P2P 配对响应
 */
export interface P2PPairingResponse {
  /** Session ID for this pairing attempt */
  sessionId: string
  /** Whether pairing was initiated successfully */
  success: boolean
  /** Error message if failed */
  error?: string
}

/**
 * P2P PIN 验证请求
 */
export interface P2PPinVerifyRequest {
  /** Session ID */
  sessionId: string
  /** Whether PIN matches */
  pinMatches: boolean
}

export type P2PPairingVerificationKind =
  | 'request'
  | 'verification'
  | 'verifying'
  | 'complete'
  | 'failed'

export type PairingErrorKind =
  | 'active_session_exists'
  | 'no_local_participant'
  | 'session_not_found'
  | 'daemon_unavailable'
  | 'unknown'

interface P2PCommandErrorEvent {
  command: string
  message: string
}

const bufferedCommandErrors = new Map<string, string[]>()
const pendingCommandErrorResolvers = new Map<string, Array<(message: string) => void>>()
let commandErrorListenerPromise: Promise<void> | null = null

function queueCommandError(command: string, message: string) {
  const pendingResolvers = pendingCommandErrorResolvers.get(command)
  if (pendingResolvers && pendingResolvers.length > 0) {
    const resolver = pendingResolvers.shift()
    if (pendingResolvers.length === 0) {
      pendingCommandErrorResolvers.delete(command)
    }
    resolver?.(message)
    return
  }

  const buffered = bufferedCommandErrors.get(command) ?? []
  buffered.push(message)
  bufferedCommandErrors.set(command, buffered)
}

async function ensureCommandErrorListener() {
  if (!commandErrorListenerPromise) {
    commandErrorListenerPromise = listen<P2PCommandErrorEvent>('p2p-command-error', event => {
      queueCommandError(event.payload.command, event.payload.message)
    })
      .then(() => undefined)
      .catch(error => {
        console.error('Failed to listen for p2p-command-error:', error)
      })
  }

  await commandErrorListenerPromise
}

function stringifyPairingError(error: unknown): string {
  if (typeof error === 'string') {
    return error
  }
  if (error instanceof Error) {
    return error.message
  }
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof error.message === 'string'
  ) {
    return error.message
  }
  return String(error)
}

async function resolveCommandErrorMessage(command: string, error: unknown): Promise<string> {
  const buffered = bufferedCommandErrors.get(command)
  if (buffered && buffered.length > 0) {
    const message = buffered.shift()
    if (buffered.length === 0) {
      bufferedCommandErrors.delete(command)
    }
    return message ?? stringifyPairingError(error)
  }

  const fallbackMessage = stringifyPairingError(error)
  return new Promise(resolve => {
    const timeoutId = globalThis.setTimeout(() => {
      const pending = pendingCommandErrorResolvers.get(command) ?? []
      pendingCommandErrorResolvers.set(
        command,
        pending.filter(candidate => candidate !== resolver)
      )
      resolve(fallbackMessage)
    }, 50)

    const resolver = (message: string) => {
      globalThis.clearTimeout(timeoutId)
      resolve(message)
    }

    const pending = pendingCommandErrorResolvers.get(command) ?? []
    pending.push(resolver)
    pendingCommandErrorResolvers.set(command, pending)
  })
}

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

/**
 * P2P 配对验证事件数据
 */
export interface P2PPairingVerificationEvent {
  /** Session ID */
  sessionId: string
  /** Event kind */
  kind: P2PPairingVerificationKind
  /** Peer ID */
  peerId?: string
  /** Device name */
  deviceName?: string
  /** Verification code (short code) */
  code?: string
  /** Local fingerprint */
  localFingerprint?: string
  /** Peer fingerprint */
  peerFingerprint?: string
  /** Error message */
  error?: string
}

/**
 * P2P 设备连接状态变化事件数据
 */
export interface P2PPeerConnectionEvent {
  /** Peer ID */
  peerId: string
  /** Device name (may be null for disconnect) */
  deviceName?: string | null
  /** Connection status */
  connected: boolean
}

/**
 * P2P 设备名称更新事件数据
 */
export interface P2PPeerNameUpdatedEvent {
  /** Peer ID */
  peerId: string
  /** Device name */
  deviceName: string
}

/**
 * P2P 设备发现状态变化事件数据
 */
export interface P2PPeerDiscoveryChangedEvent {
  /** Peer ID */
  peerId: string
  /** Device name (may be null) */
  deviceName?: string | null
  /** Discovered addresses snapshot */
  addresses: string[]
  /** true=discovered, false=lost */
  discovered: boolean
}

/**
 * Space access completion event payload.
 */
export interface SpaceAccessCompletedEvent {
  /** Session ID for idempotency and dedupe */
  sessionId: string
  /** Peer ID */
  peerId: string
  /** Whether access succeeded */
  success: boolean
  /** Optional reason when failed */
  reason?: string
  /** Event timestamp */
  ts: number
}

/**
 * Per-device sync settings content type toggles
 */
export interface ContentTypes {
  text: boolean
  image: boolean
  link: boolean
  file: boolean
  code_snippet: boolean
  rich_text: boolean
}

/**
 * Per-device sync settings (matches Rust SyncSettings serde shape)
 *
 * Field names are snake_case to match Rust serde serialization.
 * SyncFrequency enum values are lowercase ("realtime", "interval").
 */
export interface SyncSettings {
  auto_sync: boolean
  sync_frequency: 'realtime' | 'interval'
  content_types: ContentTypes
  max_file_size_mb: number
}

/**
 * Get resolved sync settings for a specific paired device.
 * Returns per-device overrides if set, otherwise global defaults.
 */
export async function getDeviceSyncSettings(peerId: string): Promise<SyncSettings> {
  try {
    return await invokeWithTrace<SyncSettings>('get_device_sync_settings', { peerId })
  } catch (error) {
    console.error('Failed to get device sync settings:', error)
    throw error
  }
}

/**
 * Update or clear per-device sync settings.
 * Passing null for settings resets to global defaults.
 */
export async function updateDeviceSyncSettings(
  peerId: string,
  settings: SyncSettings | null
): Promise<void> {
  try {
    await invokeWithTrace('update_device_sync_settings', { peerId, settings })
  } catch (error) {
    console.error('Failed to update device sync settings:', error)
    throw error
  }
}

/**
 * 获取本地 Peer ID
 */
export async function getLocalPeerId(): Promise<string> {
  try {
    return await invokeWithTrace<string>('get_local_peer_id')
  } catch (error) {
    console.error('Failed to get local peer ID:', error)
    throw error
  }
}

/**
 * 获取发现的 P2P 设备列表
 */
export async function getP2PPeers(): Promise<P2PPeerInfo[]> {
  try {
    return await invokeWithTrace<P2PPeerInfo[]>('get_p2p_peers')
  } catch (error) {
    console.error('Failed to get P2P peers:', error)
    throw error
  }
}

/**
 * 发起 P2P 配对请求
 */
export async function initiateP2PPairing(request: P2PPairingRequest): Promise<P2PPairingResponse> {
  await ensureCommandErrorListener()
  try {
    return await invokeWithTrace<P2PPairingResponse>('initiate_p2p_pairing', {
      request,
    })
  } catch (error) {
    console.error('Failed to initiate P2P pairing:', error)
    return {
      sessionId: '',
      success: false,
      error: await resolveCommandErrorMessage('initiate_p2p_pairing', error),
    }
  }
}

/**
 * 验证 PIN 并完成配对
 */
export async function verifyP2PPairingPin(request: P2PPinVerifyRequest): Promise<void> {
  await ensureCommandErrorListener()
  try {
    await invokeWithTrace('verify_p2p_pairing_pin', {
      request,
    })
  } catch (error) {
    console.error('Failed to verify P2P pairing PIN:', error)
    throw new Error(await resolveCommandErrorMessage('verify_p2p_pairing_pin', error))
  }
}

/**
 * 拒绝 P2P 配对请求
 */
export async function rejectP2PPairing(sessionId: string, peerId: string): Promise<void> {
  await ensureCommandErrorListener()
  try {
    await invokeWithTrace('reject_p2p_pairing', {
      sessionId,
      peerId,
    })
  } catch (error) {
    console.error('Failed to reject P2P pairing:', error)
    throw new Error(await resolveCommandErrorMessage('reject_p2p_pairing', error))
  }
}

/**
 * 取消 P2P 配对连接
 */
export async function unpairP2PDevice(peerId: string): Promise<void> {
  try {
    await invokeWithTrace('unpair_p2p_device', {
      peerId,
    })
  } catch (error) {
    console.error('Failed to unpair P2P device:', error)
    throw error
  }
}

/**
 * 接受 P2P 配对请求（接收方）
 */
export async function acceptP2PPairing(sessionId: string): Promise<void> {
  await ensureCommandErrorListener()
  try {
    await invokeWithTrace('accept_p2p_pairing', {
      sessionId,
    })
  } catch (error) {
    console.error('Failed to accept P2P pairing:', error)
    throw new Error(await resolveCommandErrorMessage('accept_p2p_pairing', error))
  }
}

/**
 * 监听 P2P 配对验证事件
 */
export async function onP2PPairingVerification(
  callback: (event: P2PPairingVerificationEvent) => void
): Promise<() => void> {
  return onDaemonRealtimeEvent(event => {
    if (event.topic !== 'pairing') {
      return
    }

    if (event.type === 'pairing.updated') {
      const payload = event.payload as {
        sessionId: string
        status: string
        peerId?: string
        deviceName?: string
      }

      if (payload.status === 'request' || payload.status === 'verifying') {
        callback({
          sessionId: payload.sessionId,
          kind: payload.status === 'request' ? 'request' : 'verifying',
          peerId: payload.peerId,
          deviceName: payload.deviceName,
        })
      }
      return
    }

    if (event.type === 'pairing.verificationRequired') {
      const payload = event.payload as Omit<P2PPairingVerificationEvent, 'kind'>
      callback({ ...payload, kind: 'verification' })
      return
    }

    if (event.type === 'pairing.complete') {
      const payload = event.payload as {
        sessionId: string
        peerId?: string
        deviceName?: string
      }
      callback({
        sessionId: payload.sessionId,
        peerId: payload.peerId,
        deviceName: payload.deviceName,
        kind: 'complete',
      })
      return
    }

    if (event.type === 'pairing.failed') {
      const payload = event.payload as {
        sessionId: string
        reason?: string
      }
      callback({
        sessionId: payload.sessionId,
        kind: 'failed',
        error: payload.reason,
      })
    }
  })
}

/**
 * 监听 P2P 设备连接状态变化事件
 */
export async function onP2PPeerConnectionChanged(
  callback: (event: P2PPeerConnectionEvent) => void
): Promise<() => void> {
  return onDaemonRealtimeEvent(event => {
    if (event.topic === 'peers' && event.type === 'peers.connectionChanged') {
      callback(event.payload as P2PPeerConnectionEvent)
    }
  })
}

/**
 * 监听 P2P 设备名称更新事件
 */
export async function onP2PPeerNameUpdated(
  callback: (event: P2PPeerNameUpdatedEvent) => void
): Promise<() => void> {
  return onDaemonRealtimeEvent(event => {
    if (event.topic === 'peers' && event.type === 'peers.nameUpdated') {
      callback(event.payload as P2PPeerNameUpdatedEvent)
    }
  })
}

/**
 * 监听 P2P 设备发现状态变化事件
 */
export async function onP2PPeerDiscoveryChanged(
  callback: (event: P2PPeerDiscoveryChangedEvent) => void
): Promise<() => void> {
  const knownPeers = new Map<string, { deviceName?: string | null }>()

  return onDaemonRealtimeEvent(event => {
    if (event.topic !== 'peers' || event.type !== 'peers.changed') {
      return
    }

    const payload = event.payload as {
      peers: Array<{
        peerId: string
        deviceName?: string | null
        connected: boolean
      }>
    }

    const nextPeers = new Map<string, { deviceName?: string | null }>()
    for (const peer of payload.peers) {
      nextPeers.set(peer.peerId, { deviceName: peer.deviceName ?? null })
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
      if (!nextPeers.has(peerId)) {
        callback({
          peerId,
          deviceName: previous.deviceName ?? null,
          addresses: [],
          discovered: false,
        })
      }
    }

    knownPeers.clear()
    for (const [peerId, peer] of nextPeers.entries()) {
      knownPeers.set(peerId, peer)
    }
  })
}

/**
 * 监听 Space 访问完成事件（带会话幂等过滤与去重）
 */
export async function onSpaceAccessCompleted(
  callback: (event: SpaceAccessCompletedEvent) => void
): Promise<() => void> {
  let activeSessionId: string | null = null
  const seenEventKeys = new Set<string>()

  return onDaemonRealtimeEvent(event => {
    if (event.topic !== 'setup' || event.type !== 'setup.spaceAccessCompleted') {
      return
    }

    const payload = event.payload as SpaceAccessCompletedEvent
    if (!payload.sessionId) {
      return
    }

    if (activeSessionId === null) {
      activeSessionId = payload.sessionId
    }

    if (payload.sessionId !== activeSessionId) {
      return
    }

    const dedupeKey = `${payload.sessionId}:${payload.peerId}:${payload.success}:${payload.reason ?? ''}:${payload.ts}`
    if (seenEventKeys.has(dedupeKey)) {
      return
    }
    seenEventKeys.add(dedupeKey)

    callback(payload)
    activeSessionId = null
    seenEventKeys.clear()
  })
}

/**
 * 获取本地设备信息
 */
export async function getLocalDeviceInfo(): Promise<LocalDeviceInfo> {
  try {
    return await invokeWithTrace<LocalDeviceInfo>('get_local_device_info')
  } catch (error) {
    console.error('Failed to get local device info:', error)
    throw error
  }
}

/**
 * 获取已配对的设备列表
 */
export async function getPairedPeers(): Promise<PairedPeer[]> {
  try {
    return await invokeWithTrace<PairedPeer[]>('get_paired_peers')
  } catch (error) {
    console.error('Failed to get paired peers:', error)
    throw error
  }
}

/**
 * 获取已配对的设备列表（带连接状态）
 */
export async function getPairedPeersWithStatus(): Promise<PairedPeer[]> {
  try {
    return await invokeWithTrace<PairedPeer[]>('get_paired_peers_with_status')
  } catch (error) {
    console.error('Failed to get paired peers with status:', error)
    throw error
  }
}

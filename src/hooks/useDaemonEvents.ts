/**
 * React hooks for subscribing to daemon WebSocket events.
 *
 * These hooks wrap `daemonWs.subscribe()` with React's useEffect lifecycle:
 * they subscribe on mount, re-subscribe automatically when the daemon reconnects
 * (daemonWs maintains active topics across reconnections), and unsubscribe on unmount.
 *
 * All hooks require `daemonWs` to be connected first (call `daemonWs.connect(wsUrl)`).
 */

import { useEffect, useRef } from 'react'
import { daemonWs } from '@/lib/daemon-ws'
import type { DaemonWsEvent } from '@/lib/daemon-ws'
import type { ClipboardEntryDto } from '@/api/daemon/clipboard'

// ── Payload types for daemon WS events ─────────────────────────

/** Payload for `clipboard.new-content` events. */
export interface ClipboardNewContentPayload {
  entry: ClipboardEntryDto
  origin: 'local' | 'remote'
}

/** Payload for `encryption.sessionReady` events. */
export interface EncryptionSessionReadyPayload {
  sessionId: string
}

/** Payload for `encryption.sessionFailed` events. */
export interface EncryptionSessionFailedPayload {
  reason?: string
}

// ── useClipboardNewContent ───────────────────────────────────────

/**
 * Subscribe to `clipboard.new-content` events from the daemon WebSocket.
 *
 * The daemon emits this when a new clipboard entry is created locally or synced
 * from a remote device.
 *
 * @param callback  Called with the new clipboard entry each time it arrives.
 *
 * @example
 * useClipboardNewContent((entry) => {
 *   dispatch(prependItem(transformDtoToItemResponse(entry)))
 * })
 */
export function useClipboardNewContent(callback: (entry: ClipboardEntryDto) => void): void {
  const callbackRef = useRef(callback)
  callbackRef.current = callback

  useEffect(() => {
    const handler = (event: DaemonWsEvent<ClipboardNewContentPayload>) => {
      if (event.eventType === 'clipboard.new-content') {
        callbackRef.current(event.payload.entry)
      }
    }

    const unsubscribe = daemonWs.subscribe(['clipboard'], handler)
    return unsubscribe
  }, [])
}

// ── usePairingEvents ─────────────────────────────────────────────

/** Callbacks for pairing lifecycle events. */
export interface UsePairingEventsCallbacks {
  /**
   * Called when the pairing request arrives from a remote device.
   * (kind: 'request')
   */
  onRequest?: (data: {
    sessionId: string
    peerId?: string
    deviceName?: string
  }) => void

  /**
   * Called when the user must verify the pairing with a short code.
   * (kind: 'verification')
   */
  onVerification?: (data: {
    sessionId: string
    peerId?: string
    deviceName?: string
    code?: string
    localFingerprint?: string
    peerFingerprint?: string
  }) => void

  /**
   * Called when the pairing is verifying / processing.
   * (kind: 'verifying')
   */
  onVerifying?: (data: {
    sessionId: string
    peerId?: string
    deviceName?: string
  }) => void

  /**
   * Called when the pairing completes successfully.
   * (kind: 'complete')
   */
  onComplete?: (data: {
    sessionId: string
    peerId?: string
    deviceName?: string
  }) => void

  /**
   * Called when the pairing fails.
   * (kind: 'failed')
   */
  onFailed?: (data: {
    sessionId: string
    error?: string
  }) => void
}

/**
 * Subscribe to pairing lifecycle events from the daemon WebSocket.
 *
 * Covers the full pairing state machine:
 * - `pairing.updated` (request / verifying)
 * - `pairing.verificationRequired`
 * - `pairing.complete`
 * - `pairing.failed`
 *
 * Multiple concurrent pairing sessions are supported — callbacks are invoked
 * for all events; callers should filter by `sessionId` if needed.
 *
 * @param callbacks  Object mapping event kinds to handler functions.
 *
 * @example
 * usePairingEvents({
 *   onVerification: ({ sessionId, code, deviceName }) => { ... },
 *   onComplete: ({ sessionId }) => { ... },
 *   onFailed: ({ sessionId, error }) => { ... },
 * })
 */
export function usePairingEvents(callbacks: UsePairingEventsCallbacks): void {
  const callbacksRef = useRef(callbacks)
  callbacksRef.current = callbacks

  useEffect(() => {
    const handler = (event: DaemonWsEvent) => {
      if (event.topic !== 'pairing') return

      const cbs = callbacksRef.current

      if (event.eventType === 'pairing.updated') {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const p = event.payload as any
        if (p.status === 'request' && cbs.onRequest) {
          cbs.onRequest({ sessionId: p.sessionId, peerId: p.peerId, deviceName: p.deviceName })
        } else if (p.status === 'verifying' && cbs.onVerifying) {
          cbs.onVerifying({ sessionId: p.sessionId, peerId: p.peerId, deviceName: p.deviceName })
        }
        return
      }

      if (event.eventType === 'pairing.verificationRequired') {
        if (cbs.onVerification) {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const p = event.payload as any
          cbs.onVerification({
            sessionId: p.sessionId,
            peerId: p.peerId,
            deviceName: p.deviceName,
            code: p.code,
            localFingerprint: p.localFingerprint,
            peerFingerprint: p.peerFingerprint,
          })
        }
        return
      }

      if (event.eventType === 'pairing.complete') {
        if (cbs.onComplete) {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const p = event.payload as any
          cbs.onComplete({ sessionId: p.sessionId, peerId: p.peerId, deviceName: p.deviceName })
        }
        return
      }

      if (event.eventType === 'pairing.failed') {
        if (cbs.onFailed) {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const p = event.payload as any
          cbs.onFailed({ sessionId: p.sessionId, error: p.reason })
        }
        return
      }
    }

    const unsubscribe = daemonWs.subscribe(['pairing'], handler)
    return unsubscribe
  }, [])
}

// ── useEncryptionState ───────────────────────────────────────────

/**
 * Subscribe to encryption session state events from the daemon WebSocket.
 *
 * @param onReady   Called when the encryption session becomes ready.
 * @param onFailed  Called when the encryption session fails to initialize.
 *
 * @example
 * const { encryptionReady } = useEncryptionSessionState()
 *
 * // or, for more granular control:
 * useEncryptionState({
 *   onReady: () => dispatch(setEncryptionReady(true)),
 *   onFailed: () => dispatch(setEncryptionReady(false)),
 * })
 */
export function useEncryptionState(
  onReady: () => void,
  onFailed: () => void
): void {
  const onReadyRef = useRef(onReady)
  const onFailedRef = useRef(onFailed)
  onReadyRef.current = onReady
  onFailedRef.current = onFailed

  useEffect(() => {
    const handler = (event: DaemonWsEvent) => {
      if (event.topic !== 'encryption') return

      if (event.eventType === 'encryption.sessionReady') {
        onReadyRef.current()
        return
      }

      if (event.eventType === 'encryption.sessionFailed') {
        onFailedRef.current()
        return
      }
    }

    const unsubscribe = daemonWs.subscribe(['encryption'], handler)
    return unsubscribe
  }, [])
}

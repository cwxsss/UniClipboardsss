/**
 * Daemon WS Bootstrap — connects the frontend WebSocket client to the daemon.
 *
 * Listens for the one-shot `daemon://connection-info` Tauri event (emitted once
 * by the Tauri backend when the daemon process is ready), then:
 *   1. Initializes `daemonClient` with the connection config.
 *   2. Connects `daemonWs` to the daemon's WebSocket endpoint.
 *
 * After this runs, `daemonWs` will maintain its own connection with automatic
 * reconnect (exponential backoff, max 10 attempts). All `daemonWs.subscribe()`
 * calls in hooks will automatically receive events once connected.
 */

import { listen } from '@tauri-apps/api/event'
import { daemonClient } from '@/api/daemon/client'
import { daemonWs } from '@/lib/daemon-ws'

const DAEMON_CONNECTION_EVENT = 'daemon://connection-info'

interface DaemonConnectionPayload {
  baseUrl: string
  wsUrl: string
  token: string
}

/**
 * Validates the shape of a DaemonConnectionPayload.
 * Rejects missing or empty required fields before using them to initialize clients.
 */
function validatePayload(payload: unknown): asserts payload is DaemonConnectionPayload {
  if (
    typeof payload !== 'object' ||
    payload === null ||
    !('baseUrl' in payload) ||
    !('wsUrl' in payload) ||
    !('token' in payload) ||
    typeof (payload as DaemonConnectionPayload).baseUrl !== 'string' ||
    typeof (payload as DaemonConnectionPayload).wsUrl !== 'string' ||
    typeof (payload as DaemonConnectionPayload).token !== 'string' ||
    !(payload as DaemonConnectionPayload).baseUrl ||
    !(payload as DaemonConnectionPayload).wsUrl ||
    !(payload as DaemonConnectionPayload).token
  ) {
    throw new Error('Malformed daemon connection payload: missing required fields')
  }
}

let connectionEstablished = false
let connectionPromise: Promise<void> | null = null

/**
 * Connect the frontend WebSocket client to the daemon.
 *
 * Idempotent — safe to call multiple times. Returns immediately if already connected.
 * The full bootstrap sequence is:
 *   1. Wait for `daemon://connection-info` Tauri event.
 *   2. Initialize `daemonClient` with the received connection config.
 *   3. Exchange the bearer token for a JWT session via POST /auth/connect.
 *   4. Open the WebSocket with the session token in the URL.
 *
 * WebSocket connect never starts before step 3 is complete, ensuring the daemon
 * receives an authenticated session on the first connection attempt.
 *
 * @returns Promise that resolves when the WebSocket is open,
 *          or immediately if already connected.
 */
export function connectDaemonWs(): Promise<void> {
  if (connectionEstablished) {
    return Promise.resolve()
  }

  if (connectionPromise) {
    return connectionPromise
  }

  connectionPromise = waitForConnectionEvent()
    .then(async payload => {
      // Reject malformed payloads before using them to initialize clients.
      validatePayload(payload)

      // Step 1: Initialize the HTTP client with the connection config.
      daemonClient.initialize({
        baseUrl: payload.baseUrl,
        wsUrl: payload.wsUrl,
        token: payload.token,
        pid: 0,
      })

      // Step 2: Exchange the bearer token for a JWT session before opening the WebSocket.
      // This is the critical ordering requirement — daemonWs._openSocket() reads
      // daemonClient.currentSession.token to build the auth URL, so the session
      // must exist before connect() is called.
      await daemonClient.refreshSession()

      // Step 3: Connect the WebSocket client. daemonWs will auto-reconnect on disconnect.
      await daemonWs.connect(payload.wsUrl)
    })
    .then(() => {
      connectionEstablished = true
      console.info('[daemon-ws-bootstrap] connected to daemon WebSocket')
    })
    .catch(err => {
      console.error('[daemon-ws-bootstrap] failed to connect to daemon WebSocket:', err)
      throw err
    })
    .finally(() => {
      if (!connectionEstablished) {
        connectionPromise = null
      }
    })

  return connectionPromise
}

/**
 * Reset the module-level `connectionEstablished` flag.
 * Exported for test use only — do not call in production.
 */
export function resetConnectDaemonWsForTests(): void {
  connectionEstablished = false
  connectionPromise = null
}

/**
 * Wait for the one-shot `daemon://connection-info` Tauri event.
 * Automatically unsubscribes after receiving the first event.
 */
function waitForConnectionEvent(): Promise<DaemonConnectionPayload> {
  return new Promise((resolve, reject) => {
    let unlisten: (() => void) | null = null
    let resolved = false

    listen<DaemonConnectionPayload>(DAEMON_CONNECTION_EVENT, event => {
      if (resolved) return
      resolved = true
      unlisten?.()
      resolve(event.payload)
    })
      .then(fn => {
        unlisten = fn
        if (resolved) fn()
      })
      .catch(err => {
        reject(err)
      })
  })
}

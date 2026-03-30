import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'

// ── Mocks ───────────────────────────────────────────────────────

// Mock @tauri-apps/api/event
const mockListen = vi.fn()
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => mockListen(...args),
}))

// Mock daemonClient
const mockInitialize = vi.fn()
const mockRefreshSession = vi.fn()
const mockRequest = vi.fn()
const mockDestroy = vi.fn()

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    initialize: (...args: unknown[]) => mockInitialize(...args),
    refreshSession: (...args: unknown[]) => mockRefreshSession(...args),
    request: (...args: unknown[]) => mockRequest(...args),
    destroy: (...args: unknown[]) => mockDestroy(...args),
    get initialized() {
      return true
    },
  },
}))

import { DaemonApiError, DaemonErrorCode } from '@/api/daemon/errors'
import {
  loadDaemonAuth,
  verifyAuthState,
  waitForEncryptionReady,
} from '@/lib/daemon-auth'
import type { DaemonAuthResult } from '@/lib/daemon-auth'

// ── Helpers ─────────────────────────────────────────────────────

const TEST_CONNECTION_PAYLOAD = {
  baseUrl: 'http://127.0.0.1:42715',
  wsUrl: 'ws://127.0.0.1:42715/ws',
  token: 'test-bearer-token',
}

const TEST_SESSION = {
  token: 'jwt-session-token',
  expiresAt: Date.now() + 300_000,
  encryptionReady: false,
}

/**
 * Configure mockListen to immediately invoke the callback with a payload,
 * simulating the Tauri event firing.
 */
function setupListenToEmit(payload: unknown): void {
  mockListen.mockImplementation(
    (_event: string, callback: (event: { payload: unknown }) => void) => {
      // Fire the event asynchronously to match real Tauri behavior.
      const unlisten = vi.fn()
      Promise.resolve().then(() => callback({ payload }))
      return Promise.resolve(unlisten)
    },
  )
}

// ── Setup ───────────────────────────────────────────────────────

beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers({ shouldAdvanceTime: true })
})

afterEach(() => {
  vi.useRealTimers()
})

// ── loadDaemonAuth ──────────────────────────────────────────────

describe('loadDaemonAuth', () => {
  it('listens for daemon://connection-info and initializes the client', async () => {
    setupListenToEmit(TEST_CONNECTION_PAYLOAD)
    mockRefreshSession.mockResolvedValue(TEST_SESSION)

    const result: DaemonAuthResult = await loadDaemonAuth()

    // Verify listen was called with the correct event name.
    expect(mockListen).toHaveBeenCalledWith(
      'daemon://connection-info',
      expect.any(Function),
    )

    // Verify daemonClient.initialize was called with a proper DaemonConfig.
    expect(mockInitialize).toHaveBeenCalledWith(
      expect.objectContaining({
        baseUrl: TEST_CONNECTION_PAYLOAD.baseUrl,
        wsUrl: TEST_CONNECTION_PAYLOAD.wsUrl,
        token: TEST_CONNECTION_PAYLOAD.token,
        pid: expect.any(Number),
      }),
    )

    // Verify refreshSession was called.
    expect(mockRefreshSession).toHaveBeenCalledOnce()

    // Verify result shape.
    expect(result.session).toEqual(TEST_SESSION)
    expect(result.wsUrl).toBe(TEST_CONNECTION_PAYLOAD.wsUrl)
  })

  it('propagates refreshSession errors', async () => {
    setupListenToEmit(TEST_CONNECTION_PAYLOAD)
    mockRefreshSession.mockRejectedValue(
      new DaemonApiError(DaemonErrorCode.UNAUTHORIZED, 'bad token'),
    )

    await expect(loadDaemonAuth()).rejects.toThrow('bad token')
    expect(mockInitialize).toHaveBeenCalledOnce()
  })
})

// ── verifyAuthState ─────────────────────────────────────────────

describe('verifyAuthState', () => {
  it('returns full state when daemon is healthy and encryption initialized', async () => {
    // First call: /health → ok
    mockRequest.mockResolvedValueOnce({ status: 'ok' })
    // Second call: /encryption/state → initialized + session_ready
    mockRequest.mockResolvedValueOnce({
      data: { initialized: true, sessionReady: true },
      ts: Date.now(),
    })

    const result = await verifyAuthState()

    expect(result.daemonReady).toBe(true)
    expect(result.encryptionInitialized).toBe(true)
    expect(result.encryptionSessionReady).toBe(true)

    expect(mockRequest).toHaveBeenNthCalledWith(1, '/health')
    expect(mockRequest).toHaveBeenNthCalledWith(2, '/encryption/state')
  })

  it('returns all-false when daemon is unreachable', async () => {
    mockRequest.mockRejectedValueOnce(new Error('connection refused'))

    const result = await verifyAuthState()

    expect(result.daemonReady).toBe(false)
    expect(result.encryptionInitialized).toBe(false)
    expect(result.encryptionSessionReady).toBe(false)
    // Should not attempt encryption state check if health fails.
    expect(mockRequest).toHaveBeenCalledTimes(1)
  })

  it('returns daemonReady=true but encryption=false when encryption check fails', async () => {
    mockRequest.mockResolvedValueOnce({ status: 'ok' })
    mockRequest.mockRejectedValueOnce(
      new DaemonApiError(DaemonErrorCode.UNAUTHORIZED, 'session expired'),
    )

    const result = await verifyAuthState()

    expect(result.daemonReady).toBe(true)
    expect(result.encryptionInitialized).toBe(false)
    expect(result.encryptionSessionReady).toBe(false)
  })

  it('handles health response with non-ok status', async () => {
    mockRequest.mockResolvedValueOnce({ status: 'degraded' })
    mockRequest.mockResolvedValueOnce({
      data: { initialized: false, sessionReady: false },
      ts: Date.now(),
    })

    const result = await verifyAuthState()

    expect(result.daemonReady).toBe(false)
    expect(result.encryptionInitialized).toBe(false)
  })
})

// ── waitForEncryptionReady ──────────────────────────────────────

describe('waitForEncryptionReady', () => {
  it('resolves true immediately when encryption is ready on first poll', async () => {
    mockRequest.mockResolvedValueOnce({
      data: { initialized: true, sessionReady: true },
      ts: Date.now(),
    })

    const result = await waitForEncryptionReady(5000)

    expect(result).toBe(true)
    expect(mockRequest).toHaveBeenCalledTimes(1)
    expect(mockRequest).toHaveBeenCalledWith('/encryption/state')
  })

  it('resolves true after polling when encryption becomes ready', async () => {
    // First poll: not ready
    mockRequest.mockResolvedValueOnce({
      data: { initialized: true, sessionReady: false },
      ts: Date.now(),
    })
    // Second poll: ready
    mockRequest.mockResolvedValueOnce({
      data: { initialized: true, sessionReady: true },
      ts: Date.now(),
    })

    const promise = waitForEncryptionReady(5000)

    // Advance past the 500ms poll interval.
    await vi.advanceTimersByTimeAsync(600)

    const result = await promise
    expect(result).toBe(true)
    expect(mockRequest).toHaveBeenCalledTimes(2)
  })

  it('resolves false on timeout', async () => {
    // Always return not-ready.
    mockRequest.mockResolvedValue({
      data: { initialized: true, sessionReady: false },
      ts: Date.now(),
    })

    const promise = waitForEncryptionReady(1500)

    // Advance time past the timeout.
    await vi.advanceTimersByTimeAsync(2000)

    const result = await promise
    expect(result).toBe(false)
  })

  it('ignores transient errors and keeps polling', async () => {
    // First poll: network error
    mockRequest.mockRejectedValueOnce(new Error('fetch failed'))
    // Second poll: success
    mockRequest.mockResolvedValueOnce({
      data: { initialized: true, sessionReady: true },
      ts: Date.now(),
    })

    const promise = waitForEncryptionReady(5000)

    await vi.advanceTimersByTimeAsync(600)

    const result = await promise
    expect(result).toBe(true)
    expect(mockRequest).toHaveBeenCalledTimes(2)
  })

  it('uses default timeout of 30 seconds', async () => {
    // Always not-ready.
    mockRequest.mockResolvedValue({
      data: { initialized: false, sessionReady: false },
      ts: Date.now(),
    })

    const promise = waitForEncryptionReady()

    // Advance past 30s.
    await vi.advanceTimersByTimeAsync(31_000)

    const result = await promise
    expect(result).toBe(false)
  })
})

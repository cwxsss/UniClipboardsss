/**
 * daemon-ws-bootstrap ordering and idempotency tests.
 *
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { resetDaemonConnectionInfoPollingForTests } from '@/lib/daemon-connection-info'

const TEST_PAYLOAD = {
  baseUrl: 'http://127.0.0.1:42715',
  wsUrl: 'ws://127.0.0.1:42715/ws',
  token: 'tauri-bearer-token',
}

let _sessionToken: string | null = null
let _fetchQueue: Response[] = []
let _daemonWsConnectCalls: string[] = []

const mockInvokeWithTrace = vi.fn()

function resetState(): void {
  _sessionToken = null
  _fetchQueue = []
  _daemonWsConnectCalls = []
  mockInvokeWithTrace.mockReset()
}

function authResponse(sessionToken = 'jwt-from-refresh'): Response {
  return new Response(JSON.stringify({ sessionToken, expiresInSecs: 300, refreshAtSecs: 240 }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

function errResponse(status: number): Response {
  return new Response(JSON.stringify({ error: `HTTP ${status}` }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

let refreshSessionCallCount = 0

vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: (...args: unknown[]) => mockInvokeWithTrace(...args),
}))

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    get initialized() {
      return true
    },
    get wsUrl() {
      return TEST_PAYLOAD.wsUrl
    },
    get currentSession() {
      return _sessionToken
        ? { token: _sessionToken, expiresAt: Date.now() + 300_000, encryptionReady: false }
        : null
    },
    initialize(_config: unknown) {},
    async refreshSession() {
      refreshSessionCallCount++
      const response = _fetchQueue.shift() ?? authResponse()
      if (!response.ok) throw new Error(`refreshSession failed ${response.status}`)
      const data = (await response.json()) as { sessionToken: string; expiresInSecs: number }
      _sessionToken = data.sessionToken
      return {
        token: _sessionToken,
        expiresAt: Date.now() + data.expiresInSecs * 1000,
        encryptionReady: false,
      }
    },
    destroy() {
      _sessionToken = null
      _fetchQueue = []
    },
  },
}))

vi.mock('@/lib/daemon-ws', () => ({
  daemonWs: {
    connect: vi.fn(async (url: string) => {
      _daemonWsConnectCalls.push(url)
    }),
    subscribe: vi.fn(async () => () => {}),
  },
}))

const { connectDaemonWs, resetConnectDaemonWsForTests } = await import('@/lib/daemon-ws-bootstrap')

function getDaemonWsConnectCount(): number {
  return _daemonWsConnectCalls.length
}

describe('connectDaemonWs()', () => {
  beforeEach(() => {
    resetConnectDaemonWsForTests()
    resetDaemonConnectionInfoPollingForTests()
    resetState()
    refreshSessionCallCount = 0
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('polls command until connection info is ready before refreshing session and connecting ws', async () => {
    _fetchQueue.push(authResponse('session-jwt-123'))
    mockInvokeWithTrace.mockResolvedValueOnce(null).mockResolvedValueOnce(TEST_PAYLOAD)

    const promise = connectDaemonWs()
    expect(getDaemonWsConnectCount()).toBe(0)

    await vi.advanceTimersByTimeAsync(500)
    await promise

    expect(mockInvokeWithTrace).toHaveBeenNthCalledWith(1, 'get_daemon_connection_info')
    expect(mockInvokeWithTrace).toHaveBeenNthCalledWith(2, 'get_daemon_connection_info')
    expect(refreshSessionCallCount).toBe(1)
    expect(getDaemonWsConnectCount()).toBe(1)
    expect(_daemonWsConnectCalls[0]).toBe(TEST_PAYLOAD.wsUrl)
  })

  it('shares one polling sequence across concurrent callers', async () => {
    _fetchQueue.push(authResponse())
    mockInvokeWithTrace.mockResolvedValueOnce(null).mockResolvedValueOnce(TEST_PAYLOAD)

    const first = connectDaemonWs()
    const second = connectDaemonWs()
    const third = connectDaemonWs()

    await vi.advanceTimersByTimeAsync(500)
    await Promise.all([first, second, third])

    expect(mockInvokeWithTrace).toHaveBeenCalledTimes(2)
    expect(refreshSessionCallCount).toBe(1)
    expect(getDaemonWsConnectCount()).toBe(1)
  })

  it('rejects malformed command payloads before initializing clients', async () => {
    mockInvokeWithTrace.mockResolvedValue({ baseUrl: 'http://127.0.0.1:9000' })

    await expect(connectDaemonWs()).rejects.toThrow('Malformed daemon connection payload')
    expect(getDaemonWsConnectCount()).toBe(0)
  })

  it('surfaces auth refresh failure without leaking bearer token value', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockReturnValue(undefined)
    _fetchQueue.push(errResponse(401))
    mockInvokeWithTrace.mockResolvedValue(TEST_PAYLOAD)

    await expect(connectDaemonWs()).rejects.toThrow()

    const leaks = errorSpy.mock.calls.filter(call =>
      call.some(value => typeof value === 'string' && value.includes('tauri-bearer-token'))
    )
    expect(leaks).toHaveLength(0)
    errorSpy.mockRestore()
  })
})

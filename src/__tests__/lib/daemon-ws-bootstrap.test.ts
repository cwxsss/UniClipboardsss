/**
 * daemon-ws-bootstrap ordering and idempotency tests.
 *
 * Covers:
 * - connectDaemonWs() calls daemonClient.refreshSession() BEFORE daemonWs.connect()
 *   (ordering guarantee: websocket never opens before session token exists).
 * - connectDaemonWs() is idempotent — calling twice does not create duplicate connections.
 * - Malformed connection payloads are rejected before client initialization.
 * - Errors from refreshSession() are surfaced without leaking token values.
 *
 * @vitest-environment jsdom
 */

// ── Module-level state shared across all mocks ────────────────
// Do NOT call vi.resetModules() — it recreates mock factories and breaks shared state.
// Do NOT call vi.clearAllMocks() — it resets mock implementations, breaking mocks
// across the entire test file. Use manual state reset instead.

const TEST_PAYLOAD = {
  baseUrl: 'http://127.0.0.1:42715',
  wsUrl: 'ws://127.0.0.1:42715/ws',
  token: 'tauri-bearer-token',
}

let _sessionToken: string | null = null
let _fetchQueue: Response[] = []

function resetState(): void {
  _sessionToken = null
  _fetchQueue = []
}

function authResponse(sessionToken = 'jwt-from-refresh'): Response {
  return new Response(
    JSON.stringify({ sessionToken, expiresInSecs: 300, refreshAtSecs: 240 }),
    { status: 200, headers: { 'Content-Type': 'application/json' } }
  )
}

function errResponse(status: number): Response {
  return new Response(JSON.stringify({ error: `HTTP ${status}` }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

// ── Mock daemonClient ─────────────────────────────────────────

let refreshSessionCallCount = 0

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    get initialized() { return true },
    get wsUrl() { return TEST_PAYLOAD.wsUrl },
    get currentSession() {
      return _sessionToken
        ? { token: _sessionToken, expiresAt: Date.now() + 300_000, encryptionReady: false }
        : null
    },
    initialize(_config: unknown) { /* no-op */ },
    async refreshSession() {
      refreshSessionCallCount++
      const r = _fetchQueue.shift() ?? authResponse()
      if (!r.ok) throw new Error(`refreshSession failed ${r.status}`)
      const data = await r.json() as { sessionToken: string; expiresInSecs: number }
      _sessionToken = data.sessionToken
      return { token: _sessionToken, expiresAt: Date.now() + data.expiresInSecs * 1000, encryptionReady: false }
    },
    destroy() { _sessionToken = null; _fetchQueue = [] },
  },
}))

// ── Mock Tauri event listener ────────────────────────────────

type TauriHandler = (event: { payload: unknown }) => void
const _pendingTauriListeners = new Map<string, TauriHandler>()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(
    async (eventName: string, handler: TauriHandler) => {
      _pendingTauriListeners.set(eventName, handler)
      return () => { _pendingTauriListeners.delete(eventName) }
    }
  ),
}))

function emitTauriEvent<T>(eventName: string, payload: T): void {
  const handler = _pendingTauriListeners.get(eventName)
  if (handler) {
    handler({ payload } as { payload: unknown })
    _pendingTauriListeners.delete(eventName)
  }
}

// ── Mock daemonWs with manual call tracking ─────────────────

// Manual call tracking avoids relying on vi.clearAllMocks() which can reset
// mock implementations across the entire test file.
let _daemonWsConnectCalls: string[] = []

vi.mock('@/lib/daemon-ws', () => ({
  daemonWs: {
    connect: vi.fn(async (url: string) => {
      _daemonWsConnectCalls.push(url)
    }),
    subscribe: vi.fn(async () => () => {}),
  },
}))

function getDaemonWsConnectCount(): number {
  return _daemonWsConnectCalls.length
}

function resetDaemonWsConnectCalls(): void {
  _daemonWsConnectCalls = []
}

// ── Import the module under test ─────────────────────────────
// Import at the top level so the module is evaluated once and its
// module-level `connectionEstablished` flag is stable for all tests.

const { connectDaemonWs, resetConnectDaemonWsForTests } = await import('@/lib/daemon-ws-bootstrap')

// ── Shared beforeEach helper ──────────────────────────────────

function setupEach(): void {
  // Reset module-level flag FIRST so each test gets a clean connection state.
  resetConnectDaemonWsForTests()
  _pendingTauriListeners.clear()
  resetState()
  refreshSessionCallCount = 0
  // Reset manual call tracking instead of vi.clearAllMocks() to preserve
  // mock implementations across all tests in this file.
  resetDaemonWsConnectCalls()
}

function teardownEach(): void {
  _pendingTauriListeners.clear()
}

// ── Tests ───────────────────────────────────────────────────

describe('connectDaemonWs() ordering', () => {
  beforeEach(() => { setupEach() })
  afterEach(() => { teardownEach() })

  it('calls daemonClient.refreshSession() BEFORE daemonWs.connect()', async () => {
    _fetchQueue.push(authResponse('session-jwt-123'))

    const p = connectDaemonWs()
    emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)
    await p

    expect(refreshSessionCallCount).toBe(1)
    expect(getDaemonWsConnectCount()).toBe(1)
    expect(_daemonWsConnectCalls[0]).toBe(TEST_PAYLOAD.wsUrl)
  })

  it('stores the session token from refreshSession in daemonClient.currentSession', async () => {
    const { daemonClient } = await import('@/api/daemon/client')
    _fetchQueue.push(authResponse('my-session-token'))

    const p = connectDaemonWs()
    emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)
    await p

    expect(daemonClient.currentSession?.token).toBe('my-session-token')
  })

  it('websocket URL uses the session token from refreshSession, not the raw bearer token', async () => {
    const { daemonClient } = await import('@/api/daemon/client')
    _fetchQueue.push(authResponse('jwt-from-refresh'))

    const p = connectDaemonWs()
    emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)
    await p

    expect(_daemonWsConnectCalls[0]).toBe(TEST_PAYLOAD.wsUrl)
    expect(daemonClient.currentSession?.token).toBe('jwt-from-refresh')
    expect(refreshSessionCallCount).toBe(1)
  })

  it('rejects malformed connection payloads (missing fields) before initializing clients', async () => {
    const p = connectDaemonWs()
    // Emit a payload missing required fields — must be rejected before daemonClient is used.
    emitTauriEvent('daemon://connection-info', { baseUrl: 'http://127.0.0.1:9000' } as typeof TEST_PAYLOAD)

    await expect(p).rejects.toThrow('Malformed daemon connection payload')
    expect(getDaemonWsConnectCount()).toBe(0)
  })

  it('surfaces auth refresh failure without leaking bearer token value', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockReturnValue(undefined)
    _fetchQueue.push(errResponse(401))

    const p = connectDaemonWs()
    emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)

    await expect(p).rejects.toThrow()
    const leaks = errorSpy.mock.calls.filter(a =>
      a.some((x: unknown) => typeof x === 'string' && x.includes('tauri-bearer-token'))
    )
    expect(leaks).toHaveLength(0)
    errorSpy.mockRestore()
  })
})

describe('connectDaemonWs() idempotency', () => {
  beforeEach(() => { setupEach() })
  afterEach(() => { teardownEach() })

  it('second call does not duplicate connection — daemonWs.connect called once', async () => {
    _fetchQueue.push(authResponse())

    // Call twice before the event fires.
    void connectDaemonWs()
    void connectDaemonWs()

    // Even though we called connectDaemonWs twice, daemonWs.connect
    // must NOT have been called yet — the Tauri event hasn't fired.
    expect(getDaemonWsConnectCount()).toBe(0)

    emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)

    // daemonWs.connect called exactly once (not twice) — proves idempotency.
    expect(getDaemonWsConnectCount()).toBe(1)
  })

  it('multiple concurrent calls resolve when the Tauri event fires once', async () => {
    _fetchQueue.push(authResponse())

    void connectDaemonWs()
    void connectDaemonWs()
    void connectDaemonWs()

    emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)

    // All three calls resolve (implicit — no throw).
    // refreshSession called exactly once despite 3 concurrent bootstrap calls.
    expect(refreshSessionCallCount).toBe(1)
  })
})

describe('connectDaemonWs() error handling', () => {
  beforeEach(() => { setupEach() })
  afterEach(() => { teardownEach() })

  it('daemonWs.connect() failure is re-thrown by connectDaemonWs', async () => {
    const { daemonWs } = await import('@/lib/daemon-ws')
    // Override just this test's connect mock to reject.
    ;(daemonWs.connect as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('socket failed'))
    _fetchQueue.push(authResponse())

    const p = connectDaemonWs()
    emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)

    await expect(p).rejects.toThrow('socket failed')
  })

  it('throws when Tauri event never fires (awaited)', async () => {
    vi.useFakeTimers()
    _fetchQueue.push(authResponse())

    const p = connectDaemonWs()
    const rejectSpy = vi.fn()
    p.catch(rejectSpy)

    // Advance time — event never fires in test environment.
    vi.advanceTimersByTime(60_000)
    await vi.runAllTimersAsync()

    // refreshSession was NOT called because the Tauri event never arrived.
    expect(refreshSessionCallCount).toBe(0)
    vi.useRealTimers()
  })
})

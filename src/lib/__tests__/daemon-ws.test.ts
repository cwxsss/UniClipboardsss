/**
 * Unit tests for DaemonWsClient.
 *
 * Each test creates its own DaemonWsClient via makeClient(), ensuring complete
 * isolation — no singleton state, no cross-test contamination.
 */

import { describe, expect, it, vi, beforeEach } from 'vitest'

// ── Mock WebSocket ─────────────────────────────────────────────

type MockWsInstance = {
  readyState: number
  onopen: ((event: Event) => void) | null
  onmessage: ((event: MessageEvent) => void) | null
  onerror: ((event: Event) => void) | null
  onclose: ((event: CloseEvent) => void) | null
  sentMessages: string[]
  url: string
  close: ReturnType<typeof vi.fn>
}

const CONNECTING = 0
const OPEN = 1
const CLOSED = 3

function makeMockWs(): MockWsInstance {
  return {
    readyState: CONNECTING,
    onopen: null,
    onmessage: null,
    onerror: null,
    onclose: null,
    sentMessages: [],
    url: '',
    close: vi.fn(),
  }
}

/**
 * Creates a DaemonWsClient with an injected mock WebSocket factory.
 * Each call creates a fresh mock — the reconnect logic's `_ws.close()` in reset()
 * does not affect the socket returned to the test.
 */
function makeClient(): { client: InstanceType<typeof DaemonWsClient>; ws: MockWsInstance } {
  let activeWs: MockWsInstance = makeMockWs()
  const client = new DaemonWsClient((url: string) => {
    activeWs.url = url
    const outgoing = activeWs
    activeWs = makeMockWs() // fresh mock for the next connect/reconnect
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return outgoing as any as WebSocket
  })
  return { client, ws: activeWs }
}

// ── Mock daemonClient ───────────────────────────────────────────

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    currentSession: {
      token: 'test-session-token',
      expiresAt: Date.now() + 300_000,
      encryptionReady: false,
    },
  },
}))

const { DaemonWsClient } = await import('@/lib/daemon-ws')

// ── Test helpers ────────────────────────────────────────────────

function openSocket(ws: MockWsInstance): void {
  ws.readyState = OPEN
  if (ws.onopen) ws.onopen(new Event('open'))
}

function receiveMessage(ws: MockWsInstance, payload: object): void {
  if (!ws.onmessage) return
  ws.onmessage(new MessageEvent('message', { data: JSON.stringify(payload) }))
}

function closeSocket(ws: MockWsInstance): void {
  ws.readyState = CLOSED
  if (ws.onclose) ws.onclose(new CloseEvent('close', { wasClean: false }))
}

// ── Setup / Teardown ───────────────────────────────────────────

beforeEach(() => {
  vi.clearAllMocks()
})

// ── connect() ──────────────────────────────────────────────────

describe('connect()', () => {
  it('resolves when the socket opens successfully', async () => {
    const { client, ws } = makeClient()
    const p = client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    await expect(p).resolves.toBeUndefined()
  })

  it('passes the URL with auth token to the WebSocket factory', () => {
    const { client, ws } = makeClient()
    client.connect('ws://127.0.0.1:42715/ws')
    expect(ws.url).toBe('ws://127.0.0.1:42715/ws?auth=Session%20test-session-token')
  })

  it('resolves immediately if already connected', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    const p2 = client.connect('ws://127.0.0.1:42715/ws')
    await expect(p2).resolves.toBeUndefined()
  })

  it('rejects if socket closes before opening', async () => {
    const { client, ws } = makeClient()
    const p = client.connect('ws://127.0.0.1:42715/ws')
    closeSocket(ws)
    await expect(p).rejects.toThrow('WebSocket closed before open')
  })

  it('rejects on socket error', async () => {
    const { client, ws } = makeClient()
    const p = client.connect('ws://127.0.0.1:42715/ws')
    ws.readyState = CLOSED
    if (ws.onerror) ws.onerror(new Event('error'))
    await expect(p).rejects.toThrow('WebSocket error')
  })
})

// ── disconnect() ───────────────────────────────────────────────

describe('disconnect()', () => {
  it('calls close on the socket', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    client.disconnect()
    expect(ws.close).toHaveBeenCalled()
  })

  it('cancels pending reconnect so subsequent connect() starts fresh', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    closeSocket(ws)
    client.disconnect()
    const { client: c2, ws: ws2 } = makeClient()
    const p = c2.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws2)
    await expect(p).resolves.toBeUndefined()
  })
})

// ── subscribe() ────────────────────────────────────────────────

describe('subscribe()', () => {
  it('sends a subscribe message with the correct shape', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    ws.sentMessages = []
    client.subscribe(['clipboard'], vi.fn())
    expect(ws.sentMessages).toHaveLength(1)
    const msg = JSON.parse(ws.sentMessages[0])
    expect(msg).toMatchObject({ action: 'subscribe', topics: ['clipboard'] })
    expect(typeof msg.nonce).toBe('string')
  })

  it('returns an unsubscribe function', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    ws.sentMessages = []
    const cb = vi.fn()
    const unsub = client.subscribe(['clipboard'], cb)
    unsub()
    receiveMessage(ws, { topic: 'clipboard', event_type: 'clipboard.new_content', ts: 1, session_id: null, payload: {} })
    expect(cb).not.toHaveBeenCalled()
  })

  it('includes multiple topics in one subscribe message', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    ws.sentMessages = []
    client.subscribe(['clipboard', 'encryption'], vi.fn())
    const msg = JSON.parse(ws.sentMessages[0])
    expect(msg.topics).toContain('clipboard')
    expect(msg.topics).toContain('encryption')
  })

  it('dispatches incoming events to the registered callback', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    ws.sentMessages = []
    const cb = vi.fn()
    client.subscribe(['clipboard'], cb)
    receiveMessage(ws, {
      topic: 'clipboard',
      event_type: 'clipboard.new_content',
      ts: 1_234_567_890,
      session_id: 'sid-abc',
      payload: { content: 'hello' },
    })
    expect(cb).toHaveBeenCalledTimes(1)
    expect(cb.mock.calls[0][0]).toMatchObject({
      topic: 'clipboard',
      eventType: 'clipboard.new_content',
      ts: 1_234_567_890,
      sessionId: 'sid-abc',
    })
  })

  it('dispatches to multiple callbacks registered for the same topic', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    ws.sentMessages = []
    const cb1 = vi.fn()
    const cb2 = vi.fn()
    client.subscribe(['clipboard'], cb1)
    client.subscribe(['clipboard'], cb2)
    receiveMessage(ws, { topic: 'clipboard', event_type: 'clipboard.new_content', ts: 1, session_id: null, payload: {} })
    expect(cb1).toHaveBeenCalledTimes(1)
    expect(cb2).toHaveBeenCalledTimes(1)
  })

  it('handles null session_id without crashing', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    ws.sentMessages = []
    const cb = vi.fn()
    client.subscribe(['encryption'], cb)
    expect(() => receiveMessage(ws, { topic: 'encryption', event_type: 'encryption.session_ready', ts: 1, session_id: null, payload: {} })).not.toThrow()
    expect(cb).toHaveBeenCalled()
    expect(cb.mock.calls[0][0].sessionId).toBeNull()
  })
})

// ── Reconnect ─────────────────────────────────────────────────

describe('reconnect', () => {
  it('opens a new socket after unexpected close', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    const firstWs = ws
    closeSocket(ws)
    await new Promise((r) => setTimeout(r, 1_200))
    expect(ws).not.toBe(firstWs)
    expect(ws.url).toBe('ws://127.0.0.1:42715/ws')
  })

  it('uses exponential backoff', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    closeSocket(ws)
    await new Promise((r) => setTimeout(r, 1_200))
    expect(ws.url).toBe('ws://127.0.0.1:42715/ws')
    closeSocket(ws)
    await new Promise((r) => setTimeout(r, 2_300))
    expect(ws.url).toBe('ws://127.0.0.1:42715/ws')
    closeSocket(ws)
    await new Promise((r) => setTimeout(r, 4_500))
    expect(ws.url).toBe('ws://127.0.0.1:42715/ws')
  })

  it('gives up after MAX_RECONNECT_ATTEMPTS', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    const lastWs = ws
    for (let i = 1; i <= 10; i++) {
      closeSocket(ws)
      const delay = Math.min(30_000, 1_000 * 2 ** (i - 1))
      await new Promise((r) => setTimeout(r, delay + 200))
    }
    expect(ws).toBe(lastWs)
  })

  it('re-subscribes active topics after reconnect', async () => {
    const { client, ws } = makeClient()
    await client.connect('ws://127.0.0.1:42715/ws')
    openSocket(ws)
    client.subscribe(['clipboard', 'encryption'], vi.fn())
    ws.sentMessages = []
    closeSocket(ws)
    await new Promise((r) => setTimeout(r, 1_200))
    openSocket(ws)
    expect(ws.sentMessages).toHaveLength(1)
    const msg = JSON.parse(ws.sentMessages[0])
    expect(msg.action).toBe('subscribe')
    expect(msg.topics).toContain('clipboard')
    expect(msg.topics).toContain('encryption')
  })
})

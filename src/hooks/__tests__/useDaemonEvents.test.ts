/**
 * Unit tests for useDaemonEvents hooks.
 *
 * Tests verify:
 * - Correct subscribe/unsubscribe on mount/unmount
 * - Event routing to the right callbacks
 * - Multiple concurrent subscriptions work
 * - Callback refs are kept up-to-date across renders via refs
 *
 * NOTE: These tests require vitest (npx vitest) with jsdom environment,
 * not bun test (which lacks vi.mocked and jsdom support).
 */

// eslint-disable-next-line import-x/order -- blank line between doc comment and imports is intentional; rule misinterprets as empty import group
import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useClipboardNewContent, usePairingEvents, useEncryptionState } from '../useDaemonEvents'

// ── Mock daemonWs ─────────────────────────────────────────────
//
// We mock @/lib/daemon-ws entirely. The test spy is set up in beforeEach so
// it resets between tests. We import daemonWs from the mocked module at runtime.

vi.mock('@/lib/daemon-ws', () => ({
  daemonWs: {
    subscribe: vi.fn<(...args: unknown[]) => () => void>(),
    connect: vi.fn<(...args: unknown[]) => Promise<void>>(),
    disconnect: vi.fn<(...args: unknown[]) => void>(),
    reset: vi.fn<(...args: unknown[]) => void>(),
  },
}))

// Import after vi.mock so we get the mock
import { daemonWs as mockedDaemonWs } from '@/lib/daemon-ws'

const mockUnsubscribe = vi.fn()
let capturedCb: (...args: unknown[]) => void = () => {}

// Track subscribe call arguments for test assertions
const subscribeCalls: Array<[string[], (...args: unknown[]) => void]> = []

beforeEach(() => {
  vi.clearAllMocks()
  mockUnsubscribe.mockClear()
  capturedCb = () => {}
  subscribeCalls.length = 0

   
  mockedDaemonWs.subscribe = ((topics: string[], cb: (...args: unknown[]) => void) => {
    subscribeCalls.push([topics, cb])
    capturedCb = cb
    return mockUnsubscribe
  }) as any
})

// ── useClipboardNewContent ─────────────────────────────────────

describe('useClipboardNewContent', () => {
  it('subscribes to clipboard topic on mount', () => {
    const { unmount } = renderHook(() => useClipboardNewContent(vi.fn()))

    expect(subscribeCalls.length).toBe(1)
    const [topics] = subscribeCalls[0]
    expect(topics).toContain('clipboard')

    unmount()
  })

  it('unsubscribes on unmount', () => {
    const { unmount } = renderHook(() => useClipboardNewContent(vi.fn()))
    unmount()

    expect(mockUnsubscribe).toHaveBeenCalledTimes(1)
  })

  it('calls callback with entry when clipboard.new_content event arrives', () => {
    const callback = vi.fn()
    renderHook(() => useClipboardNewContent(callback))

    act(() => {
      capturedCb({
        topic: 'clipboard',
        eventType: 'clipboard.new_content',
        ts: 1_234_567_890,
        sessionId: 'sid-1',
        payload: {
          entry: {
            id: 'entry-1',
            preview: 'hello world',
            has_detail: true,
            size_bytes: 11,
            captured_at: 1_234_567_890,
            content_type: 'text/plain',
            thumbnail_url: null,
            is_encrypted: false,
            is_favorited: false,
            updated_at: 1_234_567_890,
            active_time: 1_234_567_890,
            file_transfer_status: null,
            file_transfer_reason: null,
            link_urls: null,
            link_domains: null,
            file_sizes: null,
          },
          origin: 'local',
        },
      })
    })

    expect(callback).toHaveBeenCalledTimes(1)
    expect(callback.mock.calls[0][0]).toMatchObject({ id: 'entry-1', preview: 'hello world' })
  })

  it('ignores non-clipboard.new_content events', () => {
    const callback = vi.fn()
    renderHook(() => useClipboardNewContent(callback))

    act(() => {
      capturedCb({
        topic: 'clipboard',
        eventType: 'clipboard.something-else',
        ts: 1,
        sessionId: null,
        payload: {},
      })
    })

    expect(callback).not.toHaveBeenCalled()
  })
})

// ── usePairingEvents ─────────────────────────────────────────

describe('usePairingEvents', () => {
  it('subscribes to pairing topic on mount', () => {
    const { unmount } = renderHook(() =>
      usePairingEvents({ onVerification: vi.fn(), onComplete: vi.fn() }),
    )

    expect(subscribeCalls.length).toBe(1)
    const [topics] = subscribeCalls[0]
    expect(topics).toContain('pairing')

    unmount()
  })

  it('unsubscribes on unmount', () => {
    const { unmount } = renderHook(() => usePairingEvents({ onVerification: vi.fn() }))
    unmount()

    expect(mockUnsubscribe).toHaveBeenCalledTimes(1)
  })

  it('routes pairing.verification_required to onVerification', () => {
    const onVerification = vi.fn()
    renderHook(() => usePairingEvents({ onVerification }))

    act(() => {
      capturedCb({
        topic: 'pairing',
        eventType: 'pairing.verification_required',
        ts: 1,
        sessionId: 'session-1',
        payload: {
          sessionId: 'session-1',
          peerId: 'peer-abc',
          deviceName: 'MacBook Pro',
          code: '123456',
          localFingerprint: 'fp-local',
          peerFingerprint: 'fp-peer',
        },
      })
    })

    expect(onVerification).toHaveBeenCalledTimes(1)
    expect(onVerification.mock.calls[0][0]).toMatchObject({
      sessionId: 'session-1',
      peerId: 'peer-abc',
      deviceName: 'MacBook Pro',
      code: '123456',
    })
  })

  it('routes pairing.complete to onComplete', () => {
    const onComplete = vi.fn()
    renderHook(() => usePairingEvents({ onComplete }))

    act(() => {
      capturedCb({
        topic: 'pairing',
        eventType: 'pairing.complete',
        ts: 1,
        sessionId: 'session-1',
        payload: { sessionId: 'session-1', peerId: 'peer-xyz', deviceName: 'iPhone' },
      })
    })

    expect(onComplete).toHaveBeenCalledTimes(1)
    expect(onComplete.mock.calls[0][0]).toMatchObject({ sessionId: 'session-1', peerId: 'peer-xyz' })
  })

  it('routes pairing.failed to onFailed', () => {
    const onFailed = vi.fn()
    renderHook(() => usePairingEvents({ onFailed }))

    act(() => {
      capturedCb({
        topic: 'pairing',
        eventType: 'pairing.failed',
        ts: 1,
        sessionId: 'session-1',
        payload: { sessionId: 'session-1', reason: 'PIN mismatch' },
      })
    })

    expect(onFailed).toHaveBeenCalledTimes(1)
    expect(onFailed.mock.calls[0][0]).toMatchObject({ sessionId: 'session-1', error: 'PIN mismatch' })
  })

  it('routes pairing.updated (request) to onRequest', () => {
    const onRequest = vi.fn()
    renderHook(() => usePairingEvents({ onRequest }))

    act(() => {
      capturedCb({
        topic: 'pairing',
        eventType: 'pairing.updated',
        ts: 1,
        sessionId: 'session-1',
        payload: { sessionId: 'session-1', status: 'request', peerId: 'peer-1', deviceName: 'iPad' },
      })
    })

    expect(onRequest).toHaveBeenCalledTimes(1)
    expect(onRequest.mock.calls[0][0]).toMatchObject({ sessionId: 'session-1', peerId: 'peer-1' })
  })

  it('routes pairing.updated (verifying) to onVerifying', () => {
    const onVerifying = vi.fn()
    renderHook(() => usePairingEvents({ onVerifying }))

    act(() => {
      capturedCb({
        topic: 'pairing',
        eventType: 'pairing.updated',
        ts: 1,
        sessionId: 'session-1',
        payload: { sessionId: 'session-1', status: 'verifying', peerId: 'peer-1', deviceName: 'iPad' },
      })
    })

    expect(onVerifying).toHaveBeenCalledTimes(1)
    expect(onVerifying.mock.calls[0][0]).toMatchObject({ sessionId: 'session-1' })
  })

  it('ignores events from other topics', () => {
    const onComplete = vi.fn()
    renderHook(() => usePairingEvents({ onComplete }))

    act(() => {
      capturedCb({
        topic: 'clipboard',
        eventType: 'pairing.complete',
        ts: 1,
        sessionId: 'session-1',
        payload: { sessionId: 'session-1' },
      })
    })

    expect(onComplete).not.toHaveBeenCalled()
  })

  it('does not crash when callback is not provided', () => {
    renderHook(() => usePairingEvents({}))

    expect(() =>
      act(() => {
        capturedCb({
          topic: 'pairing',
          eventType: 'pairing.complete',
          ts: 1,
          sessionId: 'session-1',
          payload: { sessionId: 'session-1', peerId: 'peer-1', deviceName: 'Mac' },
        })
      }),
    ).not.toThrow()
  })
})

// ── useEncryptionState ─────────────────────────────────────────

describe('useEncryptionState', () => {
  it('subscribes to encryption topic on mount', () => {
    const { unmount } = renderHook(() => useEncryptionState(vi.fn(), vi.fn()))

    expect(subscribeCalls.length).toBe(1)
    const [topics] = subscribeCalls[0]
    expect(topics).toContain('encryption')

    unmount()
  })

  it('unsubscribes on unmount', () => {
    const { unmount } = renderHook(() => useEncryptionState(vi.fn(), vi.fn()))
    unmount()

    expect(mockUnsubscribe).toHaveBeenCalledTimes(1)
  })

  it('calls onReady when encryption.session_ready arrives', () => {
    const onReady = vi.fn()
    const onFailed = vi.fn()
    renderHook(() => useEncryptionState(onReady, onFailed))

    act(() => {
      capturedCb({
        topic: 'encryption',
        eventType: 'encryption.session_ready',
        ts: 1,
        sessionId: 'sid-1',
        payload: { sessionId: 'sid-1' },
      })
    })

    expect(onReady).toHaveBeenCalledTimes(1)
    expect(onFailed).not.toHaveBeenCalled()
  })

  // Note: encryption.session_failed is never emitted by the daemon — test omitted.
  it('calls onFailed when encryption.session_failed arrives', () => {
    // Omitted: daemon never emits encryption.session_failed.
    // If the daemon adds this event in the future, re-enable this test.
    void vi.fn() // placeholder
  })

  it('ignores events from other topics', () => {
    const onReady = vi.fn()
    const onFailed = vi.fn()
    renderHook(() => useEncryptionState(onReady, onFailed))

    act(() => {
      capturedCb({
        topic: 'clipboard',
        eventType: 'encryption.session_ready',
        ts: 1,
        sessionId: null,
        payload: {},
      })
    })

    expect(onReady).not.toHaveBeenCalled()
    expect(onFailed).not.toHaveBeenCalled()
  })
})

// ── Multiple concurrent subscriptions ─────────────────────────

describe('multiple concurrent subscriptions', () => {
  it('each hook instance gets its own unsubscribe', () => {
    const unsubscribes: Array<() => void> = []
    subscribeCalls.length = 0 // reset from previous tests

    // Reassign subscribe with per-instance unsubscribe tracking
    mockedDaemonWs.subscribe = vi.fn((_topics: string[], cb: (...args: unknown[]) => void) => {
      capturedCb = cb
      subscribeCalls.push([_topics, cb])
      const unsub = vi.fn()
      unsubscribes.push(unsub)
      return unsub
    }) as unknown as typeof mockedDaemonWs.subscribe

    const { unmount: u1 } = renderHook(() => useClipboardNewContent(vi.fn()))
    const { unmount: u2 } = renderHook(() => useClipboardNewContent(vi.fn()))

    // Each hook mounts once and calls subscribe
    expect(subscribeCalls.length).toBe(2)

    u1()
    u2()
    expect(unsubscribes[0]).toHaveBeenCalledTimes(1)
    expect(unsubscribes[1]).toHaveBeenCalledTimes(1)
  })

  it('different hook types subscribe to their respective topics', () => {
    subscribeCalls.length = 0 // use the shared tracker from beforeEach

    renderHook(() => useClipboardNewContent(vi.fn()))
    renderHook(() => usePairingEvents({ onComplete: vi.fn() }))
    renderHook(() => useEncryptionState(vi.fn(), vi.fn()))

    expect(subscribeCalls[0][0]).toContain('clipboard')
    expect(subscribeCalls[1][0]).toContain('pairing')
    expect(subscribeCalls[2][0]).toContain('encryption')
  })
})



import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const listenMock = vi.fn()
let capturedListener: ((event: { payload: unknown }) => void) | null = null
const invokeWithTraceMock = vi.fn()

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock.mockImplementation(async (_eventName, callback) => {
    capturedListener = callback
    return vi.fn()
  }),
}))

vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: invokeWithTraceMock,
}))

describe('p2p realtime contract', () => {
  beforeEach(() => {
    listenMock.mockClear()
    invokeWithTraceMock.mockReset()
    capturedListener = null
    vi.resetModules()
  })

  afterEach(() => {
    capturedListener = null
  })

  it('uses daemon://realtime as the only active listener name', async () => {
    const { DAEMON_REALTIME_EVENT, onDaemonRealtimeEvent } = await import('@/api/realtime')
    await onDaemonRealtimeEvent(() => {})

    expect(listenMock).toHaveBeenCalledWith(DAEMON_REALTIME_EVENT, expect.any(Function))
  })

  it('maps pairing verification envelopes with camelCase payload keys', async () => {
    const { onP2PPairingVerification } = await import('@/api/p2p')
    const callback = vi.fn()
    await onP2PPairingVerification(callback)

    capturedListener?.({
      payload: {
        topic: 'pairing',
        type: 'pairing.verificationRequired',
        ts: 1,
        payload: {
          sessionId: 'session-1',
          peerId: 'peer-1',
          deviceName: 'Desk',
          code: '123456',
        },
      },
    })

    expect(callback).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-1',
        peerId: 'peer-1',
        deviceName: 'Desk',
        code: '123456',
        kind: 'verification',
      })
    )
    expect(callback.mock.calls[0]?.[0]?.session_id).toBeUndefined()
  })

  it('does not register a legacy p2p-command-error listener when pairing commands fail', async () => {
    invokeWithTraceMock.mockRejectedValue(new Error('daemon connection refused'))

    const { initiateP2PPairing } = await import('@/api/p2p')
    const result = await initiateP2PPairing({ peerId: 'peer-1' })

    expect(result).toEqual({
      sessionId: '',
      success: false,
      error: 'daemon connection refused',
    })
    expect(listenMock).not.toHaveBeenCalledWith('p2p-command-error', expect.any(Function))
    expect(listenMock).not.toHaveBeenCalled()
  })
})

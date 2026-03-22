import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getSetupState, onSetupStateChanged } from '@/api/setup'
import { resetSetupRealtimeStoreForTests, useSetupRealtimeStore } from '@/store/setupRealtimeStore'

vi.mock('@/api/setup', () => ({
  getSetupState: vi.fn(),
  onSetupStateChanged: vi.fn(),
}))

describe('setupRealtimeStore', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetSetupRealtimeStoreForTests()
  })

  it('hydrates once and then advances from setup realtime events', async () => {
    const stopListening = vi.fn()
    let realtimeCallback:
      | ((event: {
          sessionId: string
          state: {
            JoinSpaceConfirmPeer: { short_code: string; peer_fingerprint: string; error: null }
          }
          ts: number
        }) => void)
      | null = null

    vi.mocked(getSetupState).mockResolvedValue('Welcome')
    vi.mocked(onSetupStateChanged).mockImplementation(async callback => {
      realtimeCallback = callback
      return stopListening
    })

    const { result } = renderHook(() => useSetupRealtimeStore())

    await waitFor(() => {
      expect(result.current.hydrated).toBe(true)
    })

    expect(result.current.setupState).toBe('Welcome')
    expect(result.current.sessionId).toBeNull()
    expect(getSetupState).toHaveBeenCalledTimes(1)

    act(() => {
      realtimeCallback?.({
        sessionId: 'session-setup',
        state: {
          JoinSpaceConfirmPeer: {
            short_code: '123456',
            peer_fingerprint: 'peer-fp',
            error: null,
          },
        },
        ts: 1,
      })
    })

    expect(result.current.setupState).toEqual({
      JoinSpaceConfirmPeer: {
        short_code: '123456',
        peer_fingerprint: 'peer-fp',
        error: null,
      },
    })
    expect(result.current.sessionId).toBe('session-setup')
  })

  it('applies command responses without rehydrating setup state', async () => {
    vi.mocked(getSetupState).mockResolvedValue('Welcome')
    vi.mocked(onSetupStateChanged).mockResolvedValue(() => {})

    const { result } = renderHook(() => useSetupRealtimeStore())

    await waitFor(() => {
      expect(result.current.hydrated).toBe(true)
    })

    act(() => {
      result.current.syncSetupStateFromCommand({
        ProcessingJoinSpace: { message: 'waiting for pairing verification' },
      })
    })

    expect(result.current.setupState).toEqual({
      ProcessingJoinSpace: { message: 'waiting for pairing verification' },
    })
    expect(getSetupState).toHaveBeenCalledTimes(1)
  })

  it('nulls sessionId when state transitions to Completed', async () => {
    vi.mocked(getSetupState).mockResolvedValue('Welcome')
    const stopListening = vi.fn()
    vi.mocked(onSetupStateChanged).mockImplementation(async callback => {
      // Immediately invoke callback to simulate existing session
      callback({
        sessionId: 'sess-1',
        state: {
          JoinSpaceConfirmPeer: {
            short_code: '123456',
            peer_fingerprint: 'peer-fp',
            error: null,
          },
        },
        ts: 1,
      })
      return stopListening
    })

    const { result } = renderHook(() => useSetupRealtimeStore())

    await waitFor(() => {
      expect(result.current.sessionId).toBe('sess-1')
    })

    act(() => {
      result.current.syncSetupStateFromCommand('Completed')
    })

    expect(result.current.setupState).toBe('Completed')
    expect(result.current.sessionId).toBeNull()
  })

  it('nulls sessionId when state transitions to Welcome', async () => {
    vi.mocked(getSetupState).mockResolvedValue('Welcome')
    const stopListening = vi.fn()
    vi.mocked(onSetupStateChanged).mockImplementation(async callback => {
      // Immediately invoke callback to simulate existing session
      callback({
        sessionId: 'sess-2',
        state: {
          JoinSpaceConfirmPeer: {
            short_code: '654321',
            peer_fingerprint: 'peer-fp',
            error: null,
          },
        },
        ts: 1,
      })
      return stopListening
    })

    const { result } = renderHook(() => useSetupRealtimeStore())

    await waitFor(() => {
      expect(result.current.sessionId).toBe('sess-2')
    })

    act(() => {
      result.current.syncSetupStateFromCommand('Welcome')
    })

    expect(result.current.setupState).toBe('Welcome')
    expect(result.current.sessionId).toBeNull()
  })

  it('resetSetupRealtimeStoreForTests restores default snapshot and re-hydrates', async () => {
    vi.mocked(getSetupState).mockResolvedValue(null)
    vi.mocked(onSetupStateChanged).mockResolvedValue(() => {})

    const { result } = renderHook(() => useSetupRealtimeStore())

    await waitFor(() => {
      expect(result.current.hydrated).toBe(true)
    })

    act(() => {
      resetSetupRealtimeStoreForTests()
    })

    // After reset, snapshot is cleared immediately
    expect(result.current.setupState).toBeNull()
    expect(result.current.sessionId).toBeNull()

    // After re-hydration from getSetupState(null), hydrated is true again
    await waitFor(() => {
      expect(result.current.hydrated).toBe(true)
    })
  })

  it('cleans up the realtime listener when the singleton store resets', async () => {
    const stopListening = vi.fn()

    vi.mocked(getSetupState).mockResolvedValue('Welcome')
    vi.mocked(onSetupStateChanged).mockResolvedValue(stopListening)

    renderHook(() => useSetupRealtimeStore())

    await waitFor(() => {
      expect(onSetupStateChanged).toHaveBeenCalledTimes(1)
    })

    act(() => {
      resetSetupRealtimeStoreForTests()
    })

    expect(stopListening).toHaveBeenCalledTimes(1)
  })
})

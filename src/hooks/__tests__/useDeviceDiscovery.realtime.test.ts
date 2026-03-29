// @vitest-environment jsdom

import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const getP2PPeersMock = vi.fn()
let capturedRealtimeCallback: ((event: any) => void) | null = null

vi.mock('@/api/p2p', () => ({
  getP2PPeers: (...args: unknown[]) => getP2PPeersMock(...args),
}))

vi.mock('@/api/realtime', () => ({
  onDaemonRealtimeEvent: vi.fn((callback: (event: any) => void) => {
    capturedRealtimeCallback = callback
    return Promise.resolve(() => {})
  }),
}))

describe('useDeviceDiscovery realtime', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    capturedRealtimeCallback = null
    getP2PPeersMock.mockResolvedValue([])
  })

  it('updates peer list from peers.changed envelopes', async () => {
    const { useDeviceDiscovery } = await import('@/hooks/useDeviceDiscovery')
    const { result } = renderHook(() => useDeviceDiscovery(true))

    await waitFor(() => {
      expect(getP2PPeersMock).toHaveBeenCalledTimes(1)
    })

    act(() => {
      capturedRealtimeCallback?.({
        topic: 'peers',
        type: 'peers.changed',
        ts: 1,
        payload: {
          peers: [{ peerId: 'peer-1', deviceName: 'Desk', connected: true }],
        },
      })
    })

    expect(result.current.scanPhase).toBe('hasDevices')
    expect(result.current.peers).toEqual([
      { id: 'peer-1', deviceName: 'Desk', device_type: 'desktop' },
    ])
  })

  it('applies peers.nameUpdated without re-subscribing', async () => {
    const { useDeviceDiscovery } = await import('@/hooks/useDeviceDiscovery')
    const { result } = renderHook(() => useDeviceDiscovery(true))

    await waitFor(() => {
      expect(getP2PPeersMock).toHaveBeenCalledTimes(1)
    })

    act(() => {
      capturedRealtimeCallback?.({
        topic: 'peers',
        type: 'peers.changed',
        ts: 1,
        payload: {
          peers: [{ peerId: 'peer-1', deviceName: null, connected: true }],
        },
      })
    })

    act(() => {
      capturedRealtimeCallback?.({
        topic: 'peers',
        type: 'peers.nameUpdated',
        ts: 2,
        payload: {
          peerId: 'peer-1',
          deviceName: 'Renamed Desk',
        },
      })
    })

    expect(result.current.peers[0]?.deviceName).toBe('Renamed Desk')
  })
})

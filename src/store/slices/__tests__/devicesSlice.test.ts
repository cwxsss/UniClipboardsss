import { describe, expect, it } from 'vitest'
import devicesReducer, {
  setDiscoveredPeers,
  clearDiscoveredPeers,
  updateDiscoveredPeerDeviceName,
  type DiscoveredPeer,
} from '../devicesSlice'

describe('devicesSlice discoveredPeers reducers', () => {
  describe('setDiscoveredPeers', () => {
    it('sets discoveredPeers and clears loading state', () => {
      const base = devicesReducer(undefined, { type: '@@INIT' })
      const peers: DiscoveredPeer[] = [
        { id: 'peer-1', deviceName: 'MacBook', device_type: 'desktop' },
        { id: 'peer-2', deviceName: null, device_type: 'desktop' },
      ]
      const next = devicesReducer(base, setDiscoveredPeers(peers))
      expect(next.discoveredPeers).toHaveLength(2)
      expect(next.discoveredPeersLoading).toBe(false)
    })

    it('clears previous discoveredPeers', () => {
      const withPeers = devicesReducer(
        undefined,
        setDiscoveredPeers([{ id: 'old', deviceName: 'Old', device_type: 'desktop' }])
      )
      const next = devicesReducer(withPeers, setDiscoveredPeers([]))
      expect(next.discoveredPeers).toHaveLength(0)
    })

    it('accepts functional updater to append new peers (Warning 6 fix)', () => {
      const withOne = devicesReducer(
        undefined,
        setDiscoveredPeers([{ id: 'peer-1', deviceName: 'MacBook', device_type: 'desktop' }])
      )
      const next = devicesReducer(
        withOne,
        setDiscoveredPeers((prev: DiscoveredPeer[]) => [
          ...prev,
          { id: 'peer-2', deviceName: 'iPhone', device_type: 'mobile' },
        ])
      )
      expect(next.discoveredPeers).toHaveLength(2)
      expect(next.discoveredPeers[0].id).toBe('peer-1')
      expect(next.discoveredPeers[1].id).toBe('peer-2')
    })
  })

  describe('clearDiscoveredPeers', () => {
    it('clears discoveredPeers', () => {
      const withPeer = devicesReducer(
        undefined,
        setDiscoveredPeers([{ id: 'p1', deviceName: 'Test', device_type: 'desktop' }])
      )
      const next = devicesReducer(withPeer, clearDiscoveredPeers())
      expect(next.discoveredPeers).toHaveLength(0)
    })
  })

  describe('updateDiscoveredPeerDeviceName', () => {
    it('updates deviceName for matching peer', () => {
      const withPeer = devicesReducer(
        undefined,
        setDiscoveredPeers([{ id: 'peer-1', deviceName: null, device_type: 'desktop' }])
      )
      const next = devicesReducer(
        withPeer,
        updateDiscoveredPeerDeviceName({ peerId: 'peer-1', deviceName: 'Updated Mac' })
      )
      expect(next.discoveredPeers[0].deviceName).toBe('Updated Mac')
    })

    it('does nothing for non-matching peerId', () => {
      const withPeer = devicesReducer(
        undefined,
        setDiscoveredPeers([{ id: 'peer-1', deviceName: 'Mac', device_type: 'desktop' }])
      )
      const next = devicesReducer(
        withPeer,
        updateDiscoveredPeerDeviceName({ peerId: 'peer-2', deviceName: 'Other' })
      )
      expect(next.discoveredPeers[0].deviceName).toBe('Mac')
    })
  })
})

import { describe, expect, it } from 'vitest'
import {
  classifyPairingError,
  diffPeerSnapshots,
  type PeerSnapshotPeer,
  type PeerDiffEvent,
} from '../events'

describe('classifyPairingError', () => {
  it('classifies active_session_exists', () => {
    expect(classifyPairingError('active pairing session exists')).toBe('active_session_exists')
  })

  it('classifies no_local_participant', () => {
    expect(classifyPairingError('no local pairing participant ready')).toBe('no_local_participant')
  })

  it('classifies session_not_found', () => {
    expect(classifyPairingError('pairing session not found')).toBe('session_not_found')
    expect(classifyPairingError('session expired')).toBe('session_not_found')
  })

  it('classifies daemon_unavailable', () => {
    expect(classifyPairingError('failed to connect daemon websocket')).toBe('daemon_unavailable')
  })

  it('returns unknown for unrecognized errors', () => {
    expect(classifyPairingError('some random error')).toBe('unknown')
    expect(classifyPairingError(null)).toBe('unknown')
    expect(classifyPairingError(undefined)).toBe('unknown')
  })
})

describe('diffPeerSnapshots', () => {
  it('emits discovered event for new peer', () => {
    const knownPeers = new Map<string, { deviceName?: string | null }>()
    const events: PeerDiffEvent[] = []
    const nextPeers: PeerSnapshotPeer[] = [
      { peerId: 'peer-new', deviceName: 'NewDevice', connected: true },
    ]

    diffPeerSnapshots(nextPeers, knownPeers, event => events.push(event))

    expect(events).toHaveLength(1)
    expect(events[0]).toMatchObject({ peerId: 'peer-new', discovered: true })
    expect(knownPeers.has('peer-new')).toBe(true)
  })

  it('emits lost event for removed peer', () => {
    const knownPeers = new Map<string, { deviceName?: string | null }>()
    knownPeers.set('peer-old', { deviceName: 'OldDevice' })
    const events: PeerDiffEvent[] = []

    diffPeerSnapshots([], knownPeers, event => events.push(event))

    expect(events).toHaveLength(1)
    expect(events[0]).toMatchObject({ peerId: 'peer-old', discovered: false })
  })

  it('emits no event for stable peer', () => {
    const knownPeers = new Map<string, { deviceName?: string | null }>()
    knownPeers.set('peer-stable', { deviceName: 'Stable' })
    const events: PeerDiffEvent[] = []
    const nextPeers: PeerSnapshotPeer[] = [
      { peerId: 'peer-stable', deviceName: 'Stable', connected: true },
    ]

    diffPeerSnapshots(nextPeers, knownPeers, event => events.push(event))

    expect(events).toHaveLength(0)
  })

  it('handles null deviceName', () => {
    const knownPeers = new Map<string, { deviceName?: string | null }>()
    const events: PeerDiffEvent[] = []
    const nextPeers: PeerSnapshotPeer[] = [
      { peerId: 'peer-no-name', deviceName: null, connected: true },
    ]

    diffPeerSnapshots(nextPeers, knownPeers, event => events.push(event))

    expect(events[0].deviceName).toBe(null)
    expect(knownPeers.get('peer-no-name')?.deviceName).toBe(null)
  })
})

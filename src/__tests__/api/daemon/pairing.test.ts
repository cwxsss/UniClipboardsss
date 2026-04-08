/**
 * Integration tests for the daemon pairing API module.
 *
 * Uses vi.spyOn to track daemonClient.request calls while preserving
 * the real function logic.
 *
 * Covers:
 * - GET /peers
 * - GET /paired-devices
 * - POST /pairing/initiate
 * - POST /pairing/accept
 * - POST /pairing/reject
 * - POST /pairing/sessions/{sessionId}/verify
 * - POST /pairing/unpair
 *
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { daemonClient } from '@/api/daemon/client'
import {
  getP2PPeers,
  getPairedPeers,
  getPairedPeersWithStatus,
  getLocalDeviceInfo,
  initiateP2PPairing,
  acceptP2PPairing,
  rejectP2PPairing,
  verifyP2PPairingPin,
  unpairP2PDevice,
} from '@/api/daemon/pairing'

describe('Daemon Pairing API', () => {
  let requestSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    requestSpy = vi.spyOn(daemonClient, 'request')
    requestSpy.mockResolvedValue(undefined)
  })

  afterEach(() => {
    requestSpy.mockRestore()
  })

  // ── GET /peers ─────────────────────────────────────────────

  describe('getP2PPeers()', () => {
    it('calls GET /peers', async () => {
      requestSpy.mockResolvedValueOnce([])

      await getP2PPeers()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/peers')
    })

    it('returns array of P2PPeerInfo', async () => {
      const peers = [
        {
          peerId: 'QmPeer1',
          deviceName: 'Device A',
          addresses: ['/ip4/192.168.1.1/tcp/4001'],
          isPaired: false,
          connected: true,
        },
        {
          peerId: 'QmPeer2',
          deviceName: null,
          addresses: [],
          isPaired: true,
          connected: false,
        },
      ]
      requestSpy.mockResolvedValueOnce(peers)

      const result = await getP2PPeers()

      expect(result).toHaveLength(2)
      expect(result[0].peerId).toBe('QmPeer1')
      expect(result[1].isPaired).toBe(true)
    })

    it('re-throws error on HTTP failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('peers error'))

      await expect(getP2PPeers()).rejects.toThrow('peers error')
    })
  })

  // ── GET /paired-devices ────────────────────────────────────

  describe('getPairedPeers()', () => {
    it('calls GET /paired-devices', async () => {
      requestSpy.mockResolvedValueOnce([])

      await getPairedPeers()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/paired-devices')
    })
  })

  describe('getPairedPeersWithStatus()', () => {
    it('calls GET /paired-devices (same as getPairedPeers)', async () => {
      requestSpy.mockResolvedValueOnce([])

      await getPairedPeersWithStatus()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/paired-devices')
    })
  })

  describe('getLocalDeviceInfo()', () => {
    it('unwraps the daemon response envelope from GET /device/me', async () => {
      requestSpy.mockResolvedValueOnce({
        data: {
          peerId: '12D3KooWLocalPeer',
          deviceName: 'Desk',
        },
        ts: Date.now(),
      })

      const result = await getLocalDeviceInfo()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/device/me')
      expect(result).toEqual({
        peerId: '12D3KooWLocalPeer',
        deviceName: 'Desk',
      })
    })
  })

  // ── POST /pairing/initiate ────────────────────────────────

  describe('initiateP2PPairing(request)', () => {
    it('calls POST /pairing/initiate with peerId in body', async () => {
      requestSpy.mockResolvedValueOnce({ sessionId: 'sess-abc', success: true })

      const result = await initiateP2PPairing({ peerId: 'QmTargetPeer' })

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/pairing/initiate', {
        method: 'POST',
        body: { peerId: 'QmTargetPeer' },
      })
      expect(result.sessionId).toBe('sess-abc')
      expect(result.success).toBe(true)
    })

    it('returns success=false with error on HTTP failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('initiate failed'))

      const result = await initiateP2PPairing({ peerId: 'QmPeer' })

      expect(result).toEqual({
        sessionId: '',
        success: false,
        error: 'initiate failed',
      })
    })

    it('returns success=false with error on network failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('fetch error'))

      const result = await initiateP2PPairing({ peerId: 'QmPeer' })

      expect(result).toEqual({
        sessionId: '',
        success: false,
        error: 'fetch error',
      })
    })
  })

  // ── POST /pairing/accept ────────────────────────────────

  describe('acceptP2PPairing(sessionId)', () => {
    it('calls POST /pairing/accept with sessionId in body', async () => {
      requestSpy.mockResolvedValueOnce(undefined)

      await acceptP2PPairing('session-xyz')

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/pairing/accept', {
        method: 'POST',
        body: { sessionId: 'session-xyz' },
      })
    })

    it('re-throws error on HTTP failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('accept error'))

      await expect(acceptP2PPairing('session-xyz')).rejects.toThrow('accept error')
    })
  })

  // ── POST /pairing/reject ─────────────────────────────

  describe('rejectP2PPairing(sessionId, peerId)', () => {
    it('calls POST /pairing/reject with sessionId in body (peerId ignored)', async () => {
      requestSpy.mockResolvedValueOnce(undefined)

      await rejectP2PPairing('session-abc', 'QmPeerIgnored')

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/pairing/reject', {
        method: 'POST',
        body: { sessionId: 'session-abc' },
      })
    })
  })

  // ── POST /pairing/sessions/{sessionId}/verify ─────────

  describe('verifyP2PPairingPin(sessionId, pinMatches)', () => {
    it('calls POST /pairing/sessions/{sessionId}/verify with pinMatches in body', async () => {
      requestSpy.mockResolvedValueOnce(undefined)

      await verifyP2PPairingPin('session-verify-123', true)

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/pairing/sessions/session-verify-123/verify', {
        method: 'POST',
        body: { pinMatches: true },
      })
    })

    it('handles pinMatches=false', async () => {
      requestSpy.mockResolvedValueOnce(undefined)

      await verifyP2PPairingPin('session-verify-123', false)

      expect(requestSpy).toHaveBeenCalledWith('/pairing/sessions/session-verify-123/verify', {
        method: 'POST',
        body: { pinMatches: false },
      })
    })

    it('re-throws error on HTTP failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('verify failed'))

      await expect(verifyP2PPairingPin('session-123', true)).rejects.toThrow('verify failed')
    })
  })

  // ── POST /pairing/unpair ─────────────────────────────

  describe('unpairP2PDevice(peerId)', () => {
    it('calls POST /pairing/unpair with peerId in body', async () => {
      requestSpy.mockResolvedValueOnce(undefined)

      await unpairP2PDevice('QmToUnpair')

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/pairing/unpair', {
        method: 'POST',
        body: { peerId: 'QmToUnpair' },
      })
    })

    it('re-throws error on HTTP failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('unpair error'))

      await expect(unpairP2PDevice('QmPeer')).rejects.toThrow('unpair error')
    })
  })
})

/**
 * Integration tests for the daemon setup API module.
 *
 * Uses vi.spyOn to track daemonClient.request calls while preserving
 * the real function logic (especially submitPassphrase's local mismatch check).
 *
 * Covers:
 * - GET /setup/state
 * - POST /setup/host
 * - POST /setup/join
 * - POST /setup/select-peer
 * - POST /setup/submit-passphrase
 * - POST /setup/verify-passphrase
 * - POST /setup/confirm-peer
 * - POST /setup/cancel
 *
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { daemonClient } from '@/api/daemon/client'
import {
  getSetupState,
  startNewSpace,
  startJoinSpace,
  selectJoinPeer,
  submitPassphrase,
  verifyPassphrase,
  confirmPeerTrust,
  cancelSetup,
} from '@/api/daemon/setup'

describe('Daemon Setup API', () => {
  let requestSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    // Spy on daemonClient.request to track calls while keeping real logic intact.
    requestSpy = vi.spyOn(daemonClient, 'request')
    requestSpy.mockResolvedValue(undefined)
  })

  afterEach(() => {
    requestSpy.mockRestore()
  })

  // ── GET /setup/state ─────────────────────────────────────────

  describe('getSetupState()', () => {
    it('calls GET /setup/state', async () => {
      requestSpy.mockResolvedValueOnce('Welcome')

      await getSetupState()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/state')
    })

    it('returns setup state including complex state shapes', async () => {
      const state = {
        JoinSpaceConfirmPeer: {
          short_code: 'ABC123',
          peer_fingerprint: null,
          error: null,
        },
      }
      requestSpy.mockResolvedValueOnce(state)

      const result = await getSetupState()

      expect(result).toEqual(state)
    })

    it('re-throws error on HTTP failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('server error'))

      await expect(getSetupState()).rejects.toThrow('server error')
    })
  })

  // ── POST /setup/host ─────────────────────────────────────────

  describe('startNewSpace()', () => {
    it('calls POST /setup/host', async () => {
      requestSpy.mockResolvedValueOnce({
        CreateSpaceInputPassphrase: { error: null },
      })

      await startNewSpace()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/host', { method: 'POST' })
    })

    it('re-throws error on HTTP failure', async () => {
      requestSpy.mockRejectedValueOnce(new Error('host error'))

      await expect(startNewSpace()).rejects.toThrow('host error')
    })
  })

  // ── POST /setup/join ─────────────────────────────────────────

  describe('startJoinSpace()', () => {
    it('calls POST /setup/join', async () => {
      requestSpy.mockResolvedValueOnce({
        JoinSpaceSelectDevice: { error: null },
      })

      await startJoinSpace()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/join', { method: 'POST' })
    })
  })

  // ── POST /setup/select-peer ─────────────────────────────────

  describe('selectJoinPeer(peerId)', () => {
    it('calls POST /setup/select-peer with peerId in body', async () => {
      requestSpy.mockResolvedValueOnce({
        JoinSpaceInputPassphrase: { error: null },
      })

      await selectJoinPeer('QmPeerID123')

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/select-peer', {
        method: 'POST',
        body: { peerId: 'QmPeerID123' },
      })
    })
  })

  // ── POST /setup/submit-passphrase ────────────────────────────

  describe('submitPassphrase(passphrase1, passphrase2)', () => {
    it('returns early with PassphraseMismatch when passphrases differ (no daemon call)', async () => {
      const result = await submitPassphrase('pass1', 'pass2')

      expect(result).toEqual({
        CreateSpaceInputPassphrase: { error: 'PassphraseMismatch' },
      })
      // Should NOT have called the daemon
      expect(requestSpy).not.toHaveBeenCalled()
    })

    it('calls POST /setup/submit-passphrase when passphrases match', async () => {
      requestSpy.mockResolvedValueOnce({
        ProcessingCreateSpace: { message: null },
      })

      const result = await submitPassphrase('my-secret', 'my-secret')

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/submit-passphrase', {
        method: 'POST',
        body: { passphrase: 'my-secret' },
      })
      expect(result).toEqual({ ProcessingCreateSpace: { message: null } })
    })

    it('returns PassphraseMismatch for empty strings that differ', async () => {
      const result = await submitPassphrase('', 'x')
      expect(result).toEqual({
        CreateSpaceInputPassphrase: { error: 'PassphraseMismatch' },
      })
      expect(requestSpy).not.toHaveBeenCalled()
    })

    it('re-throws error from daemon on matching passphrases', async () => {
      requestSpy.mockRejectedValueOnce(new Error('internal error'))

      await expect(submitPassphrase('same', 'same')).rejects.toThrow('internal error')
    })
  })

  // ── POST /setup/verify-passphrase ────────────────────────────

  describe('verifyPassphrase(passphrase)', () => {
    it('calls POST /setup/verify-passphrase with passphrase in body', async () => {
      requestSpy.mockResolvedValueOnce({
        ProcessingJoinSpace: { message: null },
      })

      await verifyPassphrase('join-secret')

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/verify-passphrase', {
        method: 'POST',
        body: { passphrase: 'join-secret' },
      })
    })
  })

  // ── POST /setup/confirm-peer ────────────────────────────────

  describe('confirmPeerTrust()', () => {
    it('calls POST /setup/confirm-peer', async () => {
      requestSpy.mockResolvedValueOnce({
        ProcessingJoinSpace: { message: null },
      })

      await confirmPeerTrust()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/confirm-peer', { method: 'POST' })
    })
  })

  // ── POST /setup/cancel ─────────────────────────────────────

  describe('cancelSetup()', () => {
    it('calls POST /setup/cancel', async () => {
      requestSpy.mockResolvedValueOnce('Welcome')

      await cancelSetup()

      expect(requestSpy).toHaveBeenCalledTimes(1)
      expect(requestSpy).toHaveBeenCalledWith('/setup/cancel', { method: 'POST' })
    })
  })
})

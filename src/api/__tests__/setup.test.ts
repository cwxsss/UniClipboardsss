/**
 * Tests for the setup API facade.
 * Verifies that functions delegate to daemon HTTP endpoints correctly,
 * and that submitPassphrase performs local mismatch validation before calling the daemon.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { daemonClient } from '@/api/daemon/client'
import {
  cancelSetup,
  confirmPeerTrust,
  getSetupState,
  selectJoinPeer,
  startJoinSpace,
  startNewSpace,
  submitPassphrase,
  verifyPassphrase,
} from '@/api/setup'

describe('setup api', () => {
  let requestSpy: ReturnType<typeof vi.spyOn>

  const wrapState = (state: unknown) => ({
    state,
    sessionId: null,
    nextStepHint: 'idle',
    profile: 'default',
    clipboardMode: 'full',
    deviceName: 'Test Device',
    peerId: 'peer-1',
    selectedPeerId: null,
    selectedPeerName: null,
    hasCompleted: false,
    ts: Date.now(),
  })

  beforeEach(() => {
    requestSpy = vi.spyOn(daemonClient, 'request')
    requestSpy.mockResolvedValue(undefined)
  })

  afterEach(() => {
    requestSpy.mockRestore()
  })

  it('getSetupState returns the typed setup state from daemon', async () => {
    requestSpy.mockResolvedValueOnce(
      wrapState({
        CreateSpaceInputPassphrase: { error: null },
      })
    )

    await expect(getSetupState()).resolves.toEqual({
      CreateSpaceInputPassphrase: { error: null },
    })
  })

  it('startNewSpace calls POST /setup/host', async () => {
    requestSpy.mockResolvedValueOnce(wrapState({ CreateSpaceInputPassphrase: { error: null } }))
    await startNewSpace()
    expect(requestSpy).toHaveBeenCalledTimes(1)
    expect(requestSpy).toHaveBeenCalledWith('/setup/host', { method: 'POST' })
  })

  it('startJoinSpace calls POST /setup/join', async () => {
    requestSpy.mockResolvedValueOnce(wrapState({ JoinSpaceSelectDevice: { error: null } }))
    await startJoinSpace()
    expect(requestSpy).toHaveBeenCalledTimes(1)
    expect(requestSpy).toHaveBeenCalledWith('/setup/join', { method: 'POST' })
  })

  it('confirmPeerTrust calls POST /setup/confirm-peer', async () => {
    requestSpy.mockResolvedValueOnce(wrapState({ ProcessingJoinSpace: { message: null } }))
    await confirmPeerTrust()
    expect(requestSpy).toHaveBeenCalledTimes(1)
    expect(requestSpy).toHaveBeenCalledWith('/setup/confirm-peer', { method: 'POST' })
  })

  it('cancelSetup calls POST /setup/cancel', async () => {
    requestSpy.mockResolvedValueOnce(wrapState('Welcome'))
    await cancelSetup()
    expect(requestSpy).toHaveBeenCalledTimes(1)
    expect(requestSpy).toHaveBeenCalledWith('/setup/cancel', { method: 'POST' })
  })

  it('selectJoinPeer sends peerId in body', async () => {
    requestSpy.mockResolvedValueOnce(
      wrapState({
        JoinSpaceInputPassphrase: { error: null },
      })
    )
    await selectJoinPeer('peer-abc-123')

    expect(requestSpy).toHaveBeenCalledWith('/setup/select-peer', {
      method: 'POST',
      body: { peerId: 'peer-abc-123' },
    })
  })

  it('submitPassphrase returns early with PassphraseMismatch when passphrases differ (no daemon call)', async () => {
    const result = await submitPassphrase('pass1', 'pass2')

    expect(result).toEqual({
      CreateSpaceInputPassphrase: { error: 'PassphraseMismatch' },
    })
    // Should NOT have called the daemon
    expect(requestSpy).not.toHaveBeenCalled()
  })

  it('submitPassphrase calls daemon HTTP when passphrases match', async () => {
    requestSpy.mockResolvedValueOnce(
      wrapState({
        ProcessingCreateSpace: { message: null },
      })
    )

    const result = await submitPassphrase('same-passphrase', 'same-passphrase')

    expect(requestSpy).toHaveBeenCalledWith('/setup/submit-passphrase', {
      method: 'POST',
      body: { passphrase: 'same-passphrase' },
    })
    expect(result).toEqual({
      ProcessingCreateSpace: { message: null },
    })
  })

  it('submitPassphrase returns early on passphrase1 !== passphrase2 with exact error shape', async () => {
    const mismatches = [
      ['a', 'b'],
      ['', 'x'],
      ['x', ''],
      ['hello', 'world'],
    ]

    for (const [p1, p2] of mismatches) {
      const result = await submitPassphrase(p1, p2)
      expect(result).toEqual({
        CreateSpaceInputPassphrase: { error: 'PassphraseMismatch' },
      })
    }

    // Daemon should never have been called for any mismatch
    expect(requestSpy).not.toHaveBeenCalled()
  })

  it('verifyPassphrase sends passphrase in body', async () => {
    requestSpy.mockResolvedValueOnce(
      wrapState({
        JoinSpaceInputPassphrase: { error: null },
      })
    )

    await verifyPassphrase('my-secret')

    expect(requestSpy).toHaveBeenCalledWith('/setup/verify-passphrase', {
      method: 'POST',
      body: { passphrase: 'my-secret' },
    })
  })
})

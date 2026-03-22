import { beforeEach, describe, expect, it, vi } from 'vitest'
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
import { invokeWithTrace } from '@/lib/tauri-command'

vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: vi.fn(),
}))

describe('setup api', () => {
  const invokeWithTraceMock = vi.mocked(invokeWithTrace)

  beforeEach(() => {
    invokeWithTraceMock.mockReset()
    invokeWithTraceMock.mockResolvedValue('Welcome')
  })

  it('getSetupState returns the typed setup state from tauri', async () => {
    invokeWithTraceMock.mockResolvedValue({
      CreateSpaceInputPassphrase: { error: null },
    })

    await expect(getSetupState()).resolves.toEqual({
      CreateSpaceInputPassphrase: { error: null },
    })
  })

  it.each([
    ['getSetupState', () => getSetupState(), 'get_setup_state', undefined],
    ['startNewSpace', () => startNewSpace(), 'start_new_space', undefined],
    ['startJoinSpace', () => startJoinSpace(), 'start_join_space', undefined],
    ['selectJoinPeer', () => selectJoinPeer('peer-1'), 'select_device', { peerId: 'peer-1' }],
    [
      'submitPassphrase',
      () => submitPassphrase('a', 'b'),
      'submit_passphrase',
      { passphrase1: 'a', passphrase2: 'b' },
    ],
    [
      'verifyPassphrase',
      () => verifyPassphrase('secret-passphrase'),
      'verify_passphrase',
      { passphrase: 'secret-passphrase' },
    ],
    ['confirmPeerTrust', () => confirmPeerTrust(), 'confirm_peer_trust', undefined],
    ['cancelSetup', () => cancelSetup(), 'cancel_setup', undefined],
  ])('%s calls the expected tauri command', async (_name, call, command, payload) => {
    await call()

    if (payload === undefined) {
      expect(invokeWithTraceMock).toHaveBeenCalledWith(command)
      return
    }

    expect(invokeWithTraceMock).toHaveBeenCalledWith(command, payload)
  })
})

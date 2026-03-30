import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  getEncryptionSessionStatus,
  unlockEncryptionSession,
  getEncryptionPassword,
  setEncryptionPassword,
  deleteEncryptionPassword,
} from '@/api/security'
import { invokeWithTrace } from '@/lib/tauri-command'
import { daemonClient } from '@/api/daemon/client'

// ── Mock dependencies ─────────────────────────────────────────

vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: vi.fn(),
}))

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    request: vi.fn(),
  },
}))

const invokeMock = vi.mocked(invokeWithTrace)
const daemonClientRequestMock = vi.mocked(daemonClient.request)

describe('security api', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    daemonClientRequestMock.mockReset()
  })

  describe('getEncryptionSessionStatus', () => {
    it('calls GET /encryption/state via daemonClient and returns status', async () => {
      // Daemon returns camelCase field names
      daemonClientRequestMock.mockResolvedValueOnce({
        data: {
          initialized: true,
          sessionReady: true,
        },
        ts: 1710000000000,
      })

      const result = await getEncryptionSessionStatus()

      expect(daemonClientRequestMock).toHaveBeenCalledWith('/encryption/state')
      // Returns camelCase to match daemon API
      expect(result).toEqual({ initialized: true, sessionReady: true })
    })

    it('maps sessionReady from daemon response', async () => {
      daemonClientRequestMock.mockResolvedValueOnce({
        data: {
          initialized: false,
          sessionReady: false,
        },
        ts: 1710000000000,
      })

      const result = await getEncryptionSessionStatus()

      expect(result).toEqual({ initialized: false, sessionReady: false })
    })
  })

  describe('getEncryptionPassword', () => {
    it('calls Tauri get_encryption_password command', async () => {
      invokeMock.mockResolvedValueOnce('secret-passphrase')

      const result = await getEncryptionPassword()

      expect(invokeMock).toHaveBeenCalledWith('get_encryption_password')
      expect(result).toBe('secret-passphrase')
    })
  })

  describe('setEncryptionPassword', () => {
    it('calls Tauri set_encryption_password with password', async () => {
      invokeMock.mockResolvedValueOnce(true)

      const result = await setEncryptionPassword('new-password')

      expect(invokeMock).toHaveBeenCalledWith('set_encryption_password', {
        password: 'new-password',
      })
      expect(result).toBe(true)
    })
  })

  describe('deleteEncryptionPassword', () => {
    it('calls Tauri delete_encryption_password command', async () => {
      invokeMock.mockResolvedValueOnce(true)

      const result = await deleteEncryptionPassword()

      expect(invokeMock).toHaveBeenCalledWith('delete_encryption_password')
      expect(result).toBe(true)
    })
  })

  describe('unlockEncryptionSession', () => {
    it('calls Tauri unlock_encryption_session command', async () => {
      invokeMock.mockResolvedValueOnce(true)

      const result = await unlockEncryptionSession()

      expect(invokeMock).toHaveBeenCalledWith('unlock_encryption_session')
      expect(result).toBe(true)
    })
  })
})

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { daemonClient } from '@/api/daemon/client'
import { getEncryptionSessionStatus, unlockEncryptionSession } from '@/api/security'

// ── Mock dependencies ─────────────────────────────────────────

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    request: vi.fn(),
  },
}))

const daemonClientRequestMock = vi.mocked(daemonClient.request)

describe('security api', () => {
  beforeEach(() => {
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

  describe('unlockEncryptionSession', () => {
    it('calls daemon POST /encryption/unlock and returns true on success', async () => {
      daemonClientRequestMock.mockResolvedValueOnce({
        data: { success: true },
        ts: Date.now(),
      })

      const result = await unlockEncryptionSession()

      expect(daemonClientRequestMock).toHaveBeenCalledWith(
        expect.objectContaining({ path: '/encryption/unlock' })
      )
      expect(result).toBe(true)
    })
  })
})

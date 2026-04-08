import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getStorageStats, clearCache, openDataDirectory } from '@/api/storage'

// ── Mock dependencies ─────────────────────────────────────────

const mockRequest = vi.fn()
vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    request: mockRequest,
  },
}))

const invokeMock = vi.fn()
vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: (...args: unknown[]) => invokeMock(...args),
}))

describe('storage api', () => {
  beforeEach(() => {
    mockRequest.mockReset()
    invokeMock.mockReset()
  })

  describe('getStorageStats', () => {
    it('calls GET /storage/stats via daemonClient and returns unwrapped stats', async () => {
      const mockStats = {
        totalBytes: 52428800,
        databaseBytes: 10485760,
        vaultBytes: 20971520,
        cacheBytes: 10485760,
        logsBytes: 10485760,
      }
      mockRequest.mockResolvedValueOnce({ data: mockStats, ts: Date.now() })

      const result = await getStorageStats()

      expect(mockRequest).toHaveBeenCalledWith('/storage/stats')
      expect(result).toEqual(mockStats)
    })

    it('returns correct values for empty storage', async () => {
      const emptyStats = {
        totalBytes: 0,
        databaseBytes: 0,
        vaultBytes: 0,
        cacheBytes: 0,
        logsBytes: 0,
      }
      mockRequest.mockResolvedValueOnce({ data: emptyStats, ts: Date.now() })

      const result = await getStorageStats()

      expect(result.totalBytes).toBe(0)
      expect(result.databaseBytes).toBe(0)
    })
  })

  describe('clearCache', () => {
    it('calls POST /storage/clear-cache with confirmed=true', async () => {
      mockRequest.mockResolvedValueOnce(undefined)

      await clearCache(true)

      expect(mockRequest).toHaveBeenCalledWith('/storage/clear-cache', {
        method: 'POST',
        body: { confirmed: true },
      })
    })

    it('calls POST /storage/clear-cache with confirmed=false', async () => {
      mockRequest.mockResolvedValueOnce(undefined)

      await clearCache(false)

      expect(mockRequest).toHaveBeenCalledWith('/storage/clear-cache', {
        method: 'POST',
        body: { confirmed: false },
      })
    })
  })

  describe('openDataDirectory', () => {
    it('calls Tauri open_data_directory command', async () => {
      invokeMock.mockResolvedValueOnce(undefined)

      await openDataDirectory()

      expect(invokeMock).toHaveBeenCalledWith('open_data_directory')
    })
  })
})

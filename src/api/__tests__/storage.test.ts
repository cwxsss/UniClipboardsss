import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getStorageStats, clearCache, openDataDirectory } from '@/api/storage'
import { daemonClient } from '@/api/daemon/client'

// ── Mock dependencies ─────────────────────────────────────────

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    request: vi.fn(),
  },
}))

vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: vi.fn(),
}))

const daemonClientRequestMock = vi.mocked(daemonClient.request)
const invokeMock = vi.fn()
vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: (...args: unknown[]) => invokeMock(...args),
}))

describe('storage api', () => {
  beforeEach(() => {
    daemonClientRequestMock.mockReset()
    invokeMock.mockReset()
  })

  describe('getStorageStats', () => {
    it('calls GET /storage/stats via daemonClient and returns stats', async () => {
      const mockStats = {
        total_entries: 150,
        total_size_bytes: 52428800,
        cache_size_bytes: 10485760,
        oldest_entry_ts: 1710000000000,
        newest_entry_ts: 1710090000000,
      }
      daemonClientRequestMock.mockResolvedValueOnce(mockStats)

      const result = await getStorageStats()

      expect(daemonClientRequestMock).toHaveBeenCalledWith('/storage/stats')
      expect(result).toEqual(mockStats)
    })

    it('returns correct values for empty storage', async () => {
      const emptyStats = {
        total_entries: 0,
        total_size_bytes: 0,
        cache_size_bytes: 0,
        oldest_entry_ts: null,
        newest_entry_ts: null,
      }
      daemonClientRequestMock.mockResolvedValueOnce(emptyStats)

      const result = await getStorageStats()

      expect(result.total_entries).toBe(0)
      expect(result.total_size_bytes).toBe(0)
      expect(result.oldest_entry_ts).toBeNull()
      expect(result.newest_entry_ts).toBeNull()
    })
  })

  describe('clearCache', () => {
    it('calls POST /storage/clear-cache with confirmed=true', async () => {
      daemonClientRequestMock.mockResolvedValueOnce(undefined)

      await clearCache(true)

      expect(daemonClientRequestMock).toHaveBeenCalledWith('/storage/clear-cache', {
        method: 'POST',
        body: { confirmed: true },
      })
    })

    it('calls POST /storage/clear-cache with confirmed=false', async () => {
      daemonClientRequestMock.mockResolvedValueOnce(undefined)

      await clearCache(false)

      expect(daemonClientRequestMock).toHaveBeenCalledWith('/storage/clear-cache', {
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

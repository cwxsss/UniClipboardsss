/**
 * Integration tests for DaemonClient storage API module.
 *
 * Covers:
 * - GET /storage/stats — correct shape, values
 * - POST /storage/clear-cache — missing confirmed (400), confirmed:false (400),
 *   confirmed:true (204 success)
 *
 * Error shape: DaemonApiError fields populated correctly on all error paths.
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it, beforeEach, afterEach } from 'vitest'
import { getStorageStats, clearCache } from '@/api/daemon/storage'
import { DaemonErrorCode } from '@/api/daemon/errors'
import {
  setupFetchMock,
  teardownFetchMock,
  makeStorageStatsDto,
  mockResponse,
  mockErrorResponse,
} from './_test-helpers'

describe('Storage API', () => {
  let mockFetch: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    const { mockFetch: mf } = setupFetchMock()
    mockFetch = mf
  })

  afterEach(() => {
    teardownFetchMock()
  })

  // ── GET /storage/stats ──────────────────────────────────────

  describe('getStorageStats()', () => {
    it('returns correct shape with all required fields', async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse({
          total_entries: 42,
          total_size_bytes: 524_288,
          cache_size_bytes: 131_072,
          oldest_entry_ts: 1709000000000,
          newest_entry_ts: 1710000000000,
        }),
      )

      const stats = await getStorageStats()

      expect(stats).toHaveProperty('total_entries')
      expect(stats).toHaveProperty('total_size_bytes')
      expect(stats).toHaveProperty('cache_size_bytes')
      expect(stats).toHaveProperty('oldest_entry_ts')
      expect(stats).toHaveProperty('newest_entry_ts')
      expect(stats.total_entries).toBe(42)
      expect(stats.total_size_bytes).toBe(524_288)
      expect(stats.cache_size_bytes).toBe(131_072)
      expect(stats.oldest_entry_ts).toBe(1709000000000)
      expect(stats.newest_entry_ts).toBe(1710000000000)
    })

    it('returns null timestamps when no entries exist', async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse({
          total_entries: 0,
          total_size_bytes: 0,
          cache_size_bytes: 0,
          oldest_entry_ts: null,
          newest_entry_ts: null,
        }),
      )

      const stats = await getStorageStats()

      expect(stats.total_entries).toBe(0)
      expect(stats.oldest_entry_ts).toBeNull()
      expect(stats.newest_entry_ts).toBeNull()
    })

    it('uses GET /storage/stats endpoint', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse(makeStorageStatsDto()))

      await getStorageStats()

      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/storage/stats')
    })

    it('re-throws DaemonApiError on HTTP failure', async () => {
      mockFetch.mockResolvedValueOnce(mockErrorResponse(500, { error: '500 on /storage/stats' }))

      await expect(getStorageStats()).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
      })
    })
  })

  // ── POST /storage/clear-cache ───────────────────────────────

  describe('clearCache(confirmed)', () => {
    it('sends POST with confirmed:true to /storage/clear-cache on success', async () => {
      mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }))

      await expect(clearCache(true)).resolves.toBeUndefined()

      const [url, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(url).toContain('/storage/clear-cache')
      expect((opts as { method: string }).method).toBe('POST')
      expect(JSON.parse((opts as { body: string }).body)).toEqual({ confirmed: true })
    })

    it('sends confirmed:false when explicitly called with false', async () => {
      mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }))

      await clearCache(false)

      const [, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(JSON.parse((opts as { body: string }).body)).toEqual({ confirmed: false })
    })

    it('rejects with DaemonApiError when daemon returns 400 (missing confirmed)', async () => {
      // The daemon enforces JsonRejection on the body — a body of {} or
      // { confirmed: false } returns HTTP 400.
      mockFetch.mockResolvedValueOnce(
        mockErrorResponse(400, { error: 'Missing required field: confirmed' }),
      )

      await expect(clearCache(false)).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
      })
    })

    it('error response includes details field with field-level constraint info', async () => {
      mockFetch.mockResolvedValueOnce(
        mockErrorResponse(400, {
          error: 'Missing required field: confirmed',
          field: 'confirmed',
          constraint: 'must be true',
        }),
      )

      await expect(clearCache(false)).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
        details: { field: 'confirmed', constraint: 'must be true' },
      })
    })

    it('re-throws DaemonApiError on HTTP 500 failure', async () => {
      mockFetch.mockResolvedValueOnce(
        mockErrorResponse(500, { error: '500 on /storage/clear-cache' }),
      )

      await expect(clearCache(true)).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
      })
    })
  })
})

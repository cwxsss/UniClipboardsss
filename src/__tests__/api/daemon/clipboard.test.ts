/**
 * Integration tests for DaemonClient clipboard API module.
 *
 * Covers:
 * - GET /clipboard/entries — pagination, entry shapes, not_ready status
 * - GET /clipboard/entries?id= — found, not found cases
 * - DELETE /clipboard/entries/:id — 404, success
 * - POST /clipboard/restore/:id — 404, success, already-restored
 * - PUT /clipboard/entries/:id/favorite — toggle on/off
 * - GET /clipboard/stats — correct shape and values
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it, beforeEach, afterEach } from 'vitest'
import {
  setupFetchMock,
  teardownFetchMock,
  makeEntryDto,
  mockResponse,
  mockErrorResponse,
} from './_test-helpers'
import {
  getClipboardEntries,
  getClipboardEntry,
  deleteClipboardEntry,
  restoreClipboardEntry,
  toggleFavorite,
  getClipboardStats,
} from '@/api/daemon/clipboard'
import { DaemonErrorCode } from '@/api/daemon/errors'

describe('Clipboard API', () => {
  let mockFetch: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    const { mockFetch: mf } = setupFetchMock()
    mockFetch = mf
  })

  afterEach(() => {
    teardownFetchMock()
  })

  // ── GET /clipboard/entries ──────────────────────────────────

  describe('getClipboardEntries()', () => {
    it('resolves with entries when daemon returns ready status', async () => {
      const entries = [makeEntryDto({ id: 'e1' }), makeEntryDto({ id: 'e2' })]
      mockFetch.mockResolvedValueOnce(mockResponse({ status: 'ready', entries }))

      const result = await getClipboardEntries(20, 0)

      expect(result.status).toBe('ready')
      expect(result.entries).toHaveLength(2)
      expect(mockFetch).toHaveBeenCalledTimes(1)
      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/clipboard/entries?limit=20&offset=0')
    })

    it('uses default limit=50 offset=0 when called without args', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ status: 'ready', entries: [] }))

      await getClipboardEntries()

      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/clipboard/entries?limit=50&offset=0')
    })

    it('returns entries with all expected DTO fields populated', async () => {
      const entry = makeEntryDto({
        id: 'entry-full',
        preview: 'test content',
        has_detail: true,
        size_bytes: 12,
        captured_at: 1710000000000,
        content_type: 'text/plain',
        thumbnail_url: null,
        is_encrypted: false,
        is_favorited: true,
        updated_at: 1710000001000,
        active_time: 1710000000500,
        file_transfer_status: null,
        file_transfer_reason: null,
        link_urls: null,
        link_domains: null,
        file_sizes: null,
      })
      mockFetch.mockResolvedValueOnce(mockResponse({ status: 'ready', entries: [entry] }))

      const result = await getClipboardEntries()

      const [e] = result.entries!
      expect(e.id).toBe('entry-full')
      expect(e.preview).toBe('test content')
      expect(e.has_detail).toBe(true)
      expect(e.size_bytes).toBe(12)
      expect(e.content_type).toBe('text/plain')
      expect(e.is_encrypted).toBe(false)
      expect(e.is_favorited).toBe(true)
      expect(e.file_transfer_status).toBeNull()
      expect(e.link_urls).toBeNull()
    })

    it('returns not_ready status when daemon is syncing', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ status: 'not_ready' }))

      const result = await getClipboardEntries()

      expect(result.status).toBe('not_ready')
      expect(result.entries).toBeUndefined()
    })

    it('re-throws DaemonApiError on HTTP error', async () => {
      mockFetch.mockResolvedValueOnce(
        mockErrorResponse(500, { error: '500 on /clipboard/entries' })
      )

      await expect(getClipboardEntries()).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
        message: expect.stringContaining('500'),
      })
    })
  })

  // ── GET /clipboard/entries?id= ─────────────────────────────

  describe('getClipboardEntry(id)', () => {
    it('returns the entry when found', async () => {
      const entry = makeEntryDto({ id: 'target-entry' })
      mockFetch.mockResolvedValueOnce(mockResponse({ status: 'ready', entries: [entry] }))

      const result = await getClipboardEntry('target-entry')

      expect(result).not.toBeNull()
      expect(result!.id).toBe('target-entry')
      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/clipboard/entries?id=target-entry')
    })

    it('returns null when daemon returns not_ready', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ status: 'not_ready' }))

      const result = await getClipboardEntry('some-id')

      expect(result).toBeNull()
    })

    it('returns null when entries array is empty', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ status: 'ready', entries: [] }))

      const result = await getClipboardEntry('non-existent')

      expect(result).toBeNull()
    })
  })

  // ── DELETE /clipboard/entries/:id ───────────────────────────

  describe('deleteClipboardEntry(id)', () => {
    it('resolves on 204 No Content', async () => {
      mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }))

      await expect(deleteClipboardEntry('entry-to-delete')).resolves.toBeUndefined()
      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/clipboard/entries/entry-to-delete')
    })

    it('re-throws DaemonApiError with NOT_FOUND on 404', async () => {
      mockFetch.mockResolvedValueOnce(mockErrorResponse(404, { error: 'Not found' }))

      await expect(deleteClipboardEntry('non-existent')).rejects.toMatchObject({
        code: DaemonErrorCode.NOT_FOUND,
      })
    })
  })

  // ── POST /clipboard/restore/:id ─────────────────────────────

  describe('restoreClipboardEntry(id)', () => {
    it('returns { success: true } on 200 OK', async () => {
      // handleResponse calls response.json() even on 200; provide valid JSON.
      mockFetch.mockResolvedValueOnce(mockResponse({}))

      const result = await restoreClipboardEntry('entry-to-restore')

      expect(result).toEqual({ success: true })
      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/clipboard/restore/entry-to-restore')
    })

    it('returns { success: true } even if already restored (daemon returns 200)', async () => {
      // handleResponse calls response.json() even on 200; provide valid JSON.
      mockFetch.mockResolvedValueOnce(mockResponse({}))

      const result = await restoreClipboardEntry('already-restored-entry')

      expect(result.success).toBe(true)
    })

    it('re-throws DaemonApiError with NOT_FOUND when entry does not exist', async () => {
      mockFetch.mockResolvedValueOnce(mockErrorResponse(404, { error: 'Not found' }))

      await expect(restoreClipboardEntry('missing')).rejects.toMatchObject({
        code: DaemonErrorCode.NOT_FOUND,
      })
    })
  })

  // ── POST /clipboard/entries/:id/favorite ────────────────────

  describe('toggleFavorite(id, favorited)', () => {
    it('sends POST with is_favorited:true to enable favorite', async () => {
      mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }))

      await toggleFavorite('entry-1', true)

      const [url, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(url).toContain('/clipboard/entries/entry-1/favorite')
      expect((opts as { method: string }).method).toBe('POST')
      expect(JSON.parse((opts as { body: string }).body)).toEqual({ is_favorited: true })
    })

    it('sends PUT with is_favorited:false to unfavorite', async () => {
      mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }))

      await toggleFavorite('entry-2', false)

      const [url, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(url).toContain('/clipboard/entries/entry-2/favorite')
      expect(JSON.parse((opts as { body: string }).body)).toEqual({ is_favorited: false })
    })

    it('re-throws DaemonApiError on HTTP failure', async () => {
      mockFetch.mockResolvedValueOnce(mockErrorResponse(404, { error: 'Not found' }))

      await expect(toggleFavorite('ghost', true)).rejects.toMatchObject({
        code: DaemonErrorCode.NOT_FOUND,
      })
    })
  })

  // ── GET /clipboard/stats ───────────────────────────────────

  describe('getClipboardStats()', () => {
    it('returns correct shape with total_items and total_size', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ total_items: 150, total_size: 2_500_000 }))

      const stats = await getClipboardStats()

      expect(stats).toHaveProperty('total_items')
      expect(stats).toHaveProperty('total_size')
      expect(stats.total_items).toBe(150)
      expect(stats.total_size).toBe(2_500_000)
      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/clipboard/stats')
    })

    it('returns zero counts when no entries exist', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ total_items: 0, total_size: 0 }))

      const stats = await getClipboardStats()

      expect(stats.total_items).toBe(0)
      expect(stats.total_size).toBe(0)
    })
  })
})

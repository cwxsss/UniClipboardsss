/**
 * Unit tests for the daemon clipboard API module.
 *
 * These tests verify type correctness, HTTP method contracts, and basic API function signatures.
 * Integration tests against a running daemon would require mocking the HTTP layer.
 */

import { describe, expect, it, vi } from 'vitest'
import type { ClipboardEntryDto, ClipboardEntriesResponse, ClipboardStats } from '../clipboard'

// Mock the daemonClient
vi.mock('../client', () => ({
  daemonClient: {
    request: vi.fn(),
    initialized: true,
  },
}))

// ── Type tests ───────────────────────────────────────────────────

describe('ClipboardEntryDto type', () => {
  it('accepts a valid entry projection', () => {
    const entry: ClipboardEntryDto = {
      id: 'entry-123',
      preview: 'Hello, World!',
      has_detail: true,
      size_bytes: 13,
      captured_at: 1710000000000,
      content_type: 'text/plain',
      thumbnail_url: null,
      is_encrypted: false,
      is_favorited: false,
      updated_at: 1710000000000,
      active_time: 1710000000000,
      file_transfer_status: null,
      file_transfer_reason: null,
      link_urls: null,
      link_domains: null,
      file_sizes: null,
    }

    expect(entry.id).toBe('entry-123')
    expect(entry.preview).toBe('Hello, World!')
    expect(entry.size_bytes).toBe(13)
  })

  it('accepts entry with link data', () => {
    const entry: ClipboardEntryDto = {
      id: 'entry-link-1',
      preview: 'https://example.com',
      has_detail: true,
      size_bytes: 19,
      captured_at: 1710000000000,
      content_type: 'text/uri-list',
      thumbnail_url: null,
      is_encrypted: false,
      is_favorited: true,
      updated_at: 1710000000000,
      active_time: 1710000000000,
      file_transfer_status: null,
      file_transfer_reason: null,
      link_urls: ['https://example.com/path'],
      link_domains: ['example.com'],
      file_sizes: null,
    }

    expect(entry.link_urls).toHaveLength(1)
    expect(entry.link_domains).toEqual(['example.com'])
  })

  it('accepts entry with file transfer status', () => {
    const entry: ClipboardEntryDto = {
      id: 'entry-file-1',
      preview: 'file:///path/to/document.pdf',
      has_detail: true,
      size_bytes: 102400,
      captured_at: 1710000000000,
      content_type: 'text/uri-list',
      thumbnail_url: null,
      is_encrypted: false,
      is_favorited: false,
      updated_at: 1710000000000,
      active_time: 1710000000000,
      file_transfer_status: 'completed',
      file_transfer_reason: null,
      link_urls: null,
      link_domains: null,
      file_sizes: [102400],
    }

    expect(entry.file_transfer_status).toBe('completed')
    expect(entry.file_sizes).toEqual([102400])
  })
})

describe('ClipboardEntriesResponse type', () => {
  it('accepts ready status with entries', () => {
    const response: ClipboardEntriesResponse = {
      status: 'ready',
      entries: [
        {
          id: 'entry-1',
          preview: 'Test content',
          has_detail: true,
          size_bytes: 12,
          captured_at: 1710000000000,
          content_type: 'text/plain',
          thumbnail_url: null,
          is_encrypted: false,
          is_favorited: false,
          updated_at: 1710000000000,
          active_time: 1710000000000,
          file_transfer_status: null,
          file_transfer_reason: null,
          link_urls: null,
          link_domains: null,
          file_sizes: null,
        },
      ],
    }

    expect(response.status).toBe('ready')
    expect(response.entries).toHaveLength(1)
  })

  it('accepts not_ready status', () => {
    const response: ClipboardEntriesResponse = {
      status: 'not_ready',
    }

    expect(response.status).toBe('not_ready')
    expect(response.entries).toBeUndefined()
  })
})

describe('ClipboardStats type', () => {
  it('accepts valid stats', () => {
    const stats: ClipboardStats = {
      total_items: 42,
      total_size: 1024000,
    }

    expect(stats.total_items).toBe(42)
    expect(stats.total_size).toBe(1024000)
  })

  it('accepts zero stats', () => {
    const stats: ClipboardStats = {
      total_items: 0,
      total_size: 0,
    }

    expect(stats.total_items).toBe(0)
    expect(stats.total_size).toBe(0)
  })
})

// ── API function contract tests ──────────────────────────────────

describe('toggleFavorite HTTP contract', () => {
  it('uses POST method as defined by daemon route', async () => {
    const { daemonClient } = await import('../client')
    const { toggleFavorite } = await import('../clipboard')

    // Mock successful response
    vi.mocked(daemonClient.request).mockResolvedValue(undefined)

    await toggleFavorite('entry-123', true)

    expect(daemonClient.request).toHaveBeenCalledWith(
      '/clipboard/entries/entry-123/favorite',
      expect.objectContaining({ method: 'POST' })
    )
  })

  it('sends is_favorited in request body', async () => {
    const { daemonClient } = await import('../client')
    const { toggleFavorite } = await import('../clipboard')

    vi.mocked(daemonClient.request).mockResolvedValue(undefined)

    await toggleFavorite('entry-123', true)

    expect(daemonClient.request).toHaveBeenCalledWith(
      '/clipboard/entries/entry-123/favorite',
      expect.objectContaining({
        body: { is_favorited: true },
      })
    )
  })
})

describe('clearClipboardHistory HTTP contract', () => {
  it('uses POST method to /clipboard/entries/clear', async () => {
    const { daemonClient } = await import('../client')
    const { clearClipboardHistory } = await import('../clipboard')

    vi.mocked(daemonClient.request).mockResolvedValue({
      deletedCount: 5,
      failedEntries: [],
    })

    const result = await clearClipboardHistory()

    expect(daemonClient.request).toHaveBeenCalledWith(
      '/clipboard/entries/clear',
      expect.objectContaining({ method: 'POST' })
    )
    expect(result.deletedCount).toBe(5)
    expect(result.failedEntries).toEqual([])
  })

  it('returns result with failed entries when some deletions fail', async () => {
    const { daemonClient } = await import('../client')
    const { clearClipboardHistory } = await import('../clipboard')

    vi.mocked(daemonClient.request).mockResolvedValue({
      deletedCount: 3,
      failedEntries: [['entry-4', 'database error'], ['entry-5', 'not found']],
    })

    const result = await clearClipboardHistory()

    expect(result.deletedCount).toBe(3)
    expect(result.failedEntries).toHaveLength(2)
  })
})

describe('getEntryDetail HTTP contract', () => {
  it('uses GET method to /clipboard/entries/:id', async () => {
    const { daemonClient } = await import('../client')
    const { getEntryDetail } = await import('../clipboard')

    vi.mocked(daemonClient.request).mockResolvedValue({
      id: 'entry-123',
      content: 'Hello, World!',
      sizeBytes: 13,
      createdAtMs: 1710000000000,
      activeTimeMs: 1710000000000,
      mimeType: 'text/plain',
    })

    const result = await getEntryDetail('entry-123')

    expect(daemonClient.request).toHaveBeenCalledWith(
      '/clipboard/entries/entry-123'
    )
    expect(result?.content).toBe('Hello, World!')
  })

  it('returns null on not-found error', async () => {
    const { daemonClient } = await import('../client')
    const { getEntryDetail } = await import('../clipboard')

    const notFoundError = new Error('not found')
    ;(notFoundError as { code: string }).code = 'NOT_FOUND'
    vi.mocked(daemonClient.request).mockRejectedValue(notFoundError)

    const result = await getEntryDetail('nonexistent-id')

    expect(result).toBeNull()
  })

  it('re-throws non-not-found errors', async () => {
    const { daemonClient } = await import('../client')
    const { getEntryDetail } = await import('../clipboard')

    const serverError = new Error('server error')
    vi.mocked(daemonClient.request).mockRejectedValue(serverError)

    await expect(getEntryDetail('entry-123')).rejects.toThrow('server error')
  })
})

describe('getClipboardEntryResource HTTP contract', () => {
  it('uses GET to /clipboard/entries/:id/resource', async () => {
    const { daemonClient } = await import('../client')
    const { getClipboardEntryResource } = await import('../clipboard')

    vi.mocked(daemonClient.request).mockResolvedValue({
      blob_id: 'blob-abc',
      mime_type: 'text/plain',
      size_bytes: 1024,
      url: null,
      inline_data: 'Hello World',
    })

    const result = await getClipboardEntryResource('entry-123')

    expect(daemonClient.request).toHaveBeenCalledWith(
      '/clipboard/entries/entry-123/resource'
    )
    expect(result?.blob_id).toBe('blob-abc')
  })

  it('returns null on not-found', async () => {
    const { daemonClient } = await import('../client')
    const { getClipboardEntryResource } = await import('../clipboard')

    const notFoundError = new Error('not found')
    ;(notFoundError as { code: string }).code = 'NOT_FOUND'
    vi.mocked(daemonClient.request).mockRejectedValue(notFoundError)

    const result = await getClipboardEntryResource('nonexistent-id')

    expect(result).toBeNull()
  })
})

describe('deleteClipboardEntry HTTP contract', () => {
  it('uses DELETE method', async () => {
    const { daemonClient } = await import('../client')
    const { deleteClipboardEntry } = await import('../clipboard')

    vi.mocked(daemonClient.request).mockResolvedValue(undefined)

    await deleteClipboardEntry('entry-123')

    expect(daemonClient.request).toHaveBeenCalledWith(
      '/clipboard/entries/entry-123',
      expect.objectContaining({ method: 'DELETE' })
    )
  })
})

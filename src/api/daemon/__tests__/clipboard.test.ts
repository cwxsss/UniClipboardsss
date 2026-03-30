/**
 * Unit tests for the daemon clipboard API module.
 *
 * These tests verify type correctness and basic API function signatures.
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

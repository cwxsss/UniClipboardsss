// Mock window.matchMedia for useThemeSync
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation(query => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
})

// Mock Tauri event listener
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, _callback: (event: { payload: unknown }) => void) => {
    capturedListeners[event] = _callback
    return Promise.resolve(() => { delete capturedListeners[event] })
  }),
}))

// Mock daemon clipboard API
vi.mock('@/api/daemon/clipboard', () => ({
  getClipboardEntryResource: vi.fn(),
  getClipboardEntryDetail: vi.fn(),
}))

// Mock useThemeSync to avoid daemon client initialization
vi.mock('@/hooks/useThemeSync', () => ({ useThemeSync: vi.fn() }))

// Mock protocol utility
vi.mock('@/lib/protocol', () => ({ resolveUcUrl: vi.fn((url: string) => url) }))

const capturedListeners: Record<string, (event: { payload: unknown }) => void> = {}

import { render, screen, waitFor, act } from '@testing-library/react'
import { describe, expect, it, vi, beforeEach } from 'vitest'
import PreviewPanel from '../PreviewPanel'

describe('PreviewPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    Object.keys(capturedListeners).forEach(key => delete capturedListeners[key])
  })

  it('renders empty state initially', () => {
    render(<PreviewPanel />)
    expect(screen.getByText('Hover over an item to preview')).toBeInTheDocument()
  })

  it('shows loading spinner when preview-panel://show event is received', async () => {
    const { getClipboardEntryResource } = await import('@/api/daemon/clipboard')
    vi.mocked(getClipboardEntryResource).mockReturnValue(new Promise(() => {}))

    render(<PreviewPanel />)

    const showHandler = capturedListeners['preview-panel://show']
    act(() => { showHandler({ payload: { entryId: 'test-entry-1' } }) })

    expect(screen.getByRole('img')).toBeInTheDocument()
  })

  it('displays text content from getClipboardEntryDetail', async () => {
    const { getClipboardEntryResource, getClipboardEntryDetail } = await import('@/api/daemon/clipboard')
    vi.mocked(getClipboardEntryResource).mockResolvedValue({
      blob_id: null, mime_type: 'text/plain', size_bytes: 13, url: null, inline_data: null,
    })
    vi.mocked(getClipboardEntryDetail).mockResolvedValue({
      id: 'test-entry-1', content: 'Hello, World!', sizeBytes: 13,
      createdAtMs: 1710000000000, activeTimeMs: 1710000000000, mimeType: 'text/plain',
    })

    render(<PreviewPanel />)

    const showHandler = capturedListeners['preview-panel://show']
    act(() => { showHandler({ payload: { entryId: 'test-entry-1' } }) })

    await waitFor(() => {
      expect(screen.getByText('Hello, World!')).toBeInTheDocument()
    })
  })

  it('displays image content from getClipboardEntryResource', async () => {
    const { getClipboardEntryResource } = await import('@/api/daemon/clipboard')
    vi.mocked(getClipboardEntryResource).mockResolvedValue({
      blob_id: 'blob-123', mime_type: 'image/png', size_bytes: 1024,
      url: 'http://localhost/blob/123', inline_data: null,
    })

    render(<PreviewPanel />)

    const showHandler = capturedListeners['preview-panel://show']
    act(() => { showHandler({ payload: { entryId: 'test-entry-2' } }) })

    await waitFor(() => {
      expect(screen.getByRole('img')).toBeInTheDocument()
    })
  })

  it('handles getClipboardEntryResource error gracefully', async () => {
    const { getClipboardEntryResource } = await import('@/api/daemon/clipboard')
    vi.mocked(getClipboardEntryResource).mockRejectedValue(new Error('Network error'))

    render(<PreviewPanel />)

    const showHandler = capturedListeners['preview-panel://show']
    act(() => { showHandler({ payload: { entryId: 'test-entry-3' } }) })

    await waitFor(() => {
      expect(screen.getByText('Failed to load preview')).toBeInTheDocument()
    })
  })

  it('clears preview on preview-panel://hide event', async () => {
    const { getClipboardEntryResource, getClipboardEntryDetail } = await import('@/api/daemon/clipboard')
    vi.mocked(getClipboardEntryResource).mockResolvedValue({
      blob_id: null, mime_type: 'text/plain', size_bytes: 13, url: null, inline_data: null,
    })
    vi.mocked(getClipboardEntryDetail).mockResolvedValue({
      id: 'test-entry-1', content: 'Hello, World!', sizeBytes: 13,
      createdAtMs: 1710000000000, activeTimeMs: 1710000000000, mimeType: 'text/plain',
    })

    render(<PreviewPanel />)

    const showHandler = capturedListeners['preview-panel://show']
    act(() => { showHandler({ payload: { entryId: 'test-entry-1' } }) })
    await waitFor(() => { expect(screen.getByText('Hello, World!')).toBeInTheDocument() })

    const hideHandler = capturedListeners['preview-panel://hide']
    act(() => { hideHandler({ payload: {} }) })

    expect(screen.getByText('Hover over an item to preview')).toBeInTheDocument()
  })
})

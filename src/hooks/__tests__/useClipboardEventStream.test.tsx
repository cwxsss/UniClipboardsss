import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useClipboardEventStream } from '../useClipboardEventStream'
import { getClipboardEntry } from '@/api/clipboardItems'

// Mock daemonWs instead of Tauri listen (hook now uses daemonWs.subscribe)
vi.mock('@/lib/daemon-ws', () => ({
  daemonWs: {
    subscribe: vi.fn((_topics, handler) => {
      // Capture the handler so tests can invoke it directly.
      capturedHandler = handler
      return () => {
        capturedHandler = null
      }
    }),
  },
}))

// Mock clipboard API
vi.mock('@/api/clipboardItems', async importOriginal => {
  const actual = await importOriginal<typeof import('@/api/clipboardItems')>()
  return {
    ...actual,
    getClipboardEntry: vi.fn(),
  }
})

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let capturedHandler: ((event: any) => void) | null = null
const mockGetClipboardEntry = vi.mocked(getClipboardEntry)

describe('useClipboardEventStream', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    capturedHandler = null
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('loads single local item and emits onLocalItem', async () => {
    const onLocalItem = vi.fn()
    mockGetClipboardEntry.mockResolvedValue({
      id: 'entry-1',
      is_downloaded: true,
      is_favorited: false,
      created_at: 0,
      updated_at: 0,
      active_time: 0,
      item: { text: { display_text: 'hello', has_detail: false, size: 5 } },
    } as never)

    renderHook(() =>
      useClipboardEventStream({
        onLocalItem,
        onRemoteInvalidate: vi.fn(),
        onDeleted: vi.fn(),
      })
    )

    await waitFor(() => expect(capturedHandler).not.toBeNull())

    await act(async () => {
      capturedHandler?.({
        topic: 'clipboard',
        eventType: 'clipboard.new-content',
        ts: 0,
        sessionId: null,
        payload: { entry_id: 'entry-1', preview: 'hello', origin: 'local' },
      })
      await Promise.resolve()
    })

    expect(mockGetClipboardEntry).toHaveBeenCalledWith('entry-1')
    expect(onLocalItem).toHaveBeenCalledWith(expect.objectContaining({ id: 'entry-1' }))
  })

  it('throttles remote invalidation', async () => {
    const onRemoteInvalidate = vi.fn()

    renderHook(() =>
      useClipboardEventStream({
        onLocalItem: vi.fn(),
        onRemoteInvalidate,
        onDeleted: vi.fn(),
      })
    )

    await waitFor(() => expect(capturedHandler).not.toBeNull())
    vi.useFakeTimers()

    act(() => {
      capturedHandler?.({
        topic: 'clipboard',
        eventType: 'clipboard.new-content',
        ts: 0,
        sessionId: null,
        payload: { entry_id: 'entry-1', preview: '...', origin: 'remote' },
      })
      capturedHandler?.({
        topic: 'clipboard',
        eventType: 'clipboard.new-content',
        ts: 0,
        sessionId: null,
        payload: { entry_id: 'entry-2', preview: '...', origin: 'remote' },
      })
    })

    expect(onRemoteInvalidate).toHaveBeenCalledTimes(1)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300)
    })

    expect(onRemoteInvalidate).toHaveBeenCalledTimes(2)
    vi.useRealTimers()
  })

  it('forwards delete events', async () => {
    const onDeleted = vi.fn()

    renderHook(() =>
      useClipboardEventStream({
        onLocalItem: vi.fn(),
        onRemoteInvalidate: vi.fn(),
        onDeleted,
      })
    )

    await waitFor(() => expect(capturedHandler).not.toBeNull())

    act(() => {
      capturedHandler?.({
        topic: 'clipboard',
        eventType: 'clipboard.deleted',
        ts: 0,
        sessionId: null,
        payload: { entry_id: 'entry-9' },
      })
    })

    expect(onDeleted).toHaveBeenCalledWith('entry-9')
  })
})

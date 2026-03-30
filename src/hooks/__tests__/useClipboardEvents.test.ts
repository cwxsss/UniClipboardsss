import { configureStore } from '@reduxjs/toolkit'
import { renderHook, act } from '@testing-library/react'
import React from 'react'
import { Provider } from 'react-redux'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useClipboardEvents } from '../useClipboardEvents'
import { Filter, getClipboardEntry } from '@/api/clipboardItems'
import { getEncryptionSessionStatus } from '@/api/security'
import { invokeWithTrace } from '@/lib/tauri-command'
import clipboardReducer from '@/store/slices/clipboardSlice'

// Mock daemonWs (replaces Tauri listen — useClipboardEventStream now uses daemonWs.subscribe)
let capturedClipboardHandler: ((event: unknown) => void) | null = null
vi.mock('@/lib/daemon-ws', () => ({
  daemonWs: {
    subscribe: vi.fn((topics: string[], handler: (event: unknown) => void) => {
      if (topics.includes('clipboard')) {
        capturedClipboardHandler = handler
      }
      return () => {
        if (topics.includes('clipboard')) {
          capturedClipboardHandler = null
        }
      }
    }),
  },
}))

// Mock clipboard API (getClipboardEntries is called inside fetchClipboardItems thunk)
vi.mock('@/api/clipboardItems', async importOriginal => {
  const actual = await importOriginal<typeof import('@/api/clipboardItems')>()
  return {
    ...actual,
    getClipboardEntry: vi.fn(),
    // getClipboardEntries is imported inside clipboardSlice via @/api/daemon/clipboard
    // We need to mock it so fetchClipboardItems succeeds
  }
})

// Also mock the daemon clipboard module directly (used by fetchClipboardItems)
vi.mock('@/api/daemon/clipboard', () => ({
  getClipboardEntries: vi.fn().mockResolvedValue({ items: [], status: 'ready' }),
}))

// Mock security API
vi.mock('@/api/security', () => ({
  getEncryptionSessionStatus: vi.fn().mockResolvedValue({
    initialized: false,
    session_ready: false,
  }),
}))

// Mock toast
vi.mock('@/components/ui/toast', () => ({
  toast: { error: vi.fn() },
}))

// Mock daemonClient.request so that getClipboardEntries succeeds inside fetchClipboardItems
// (fetchClipboardItems calls getClipboardEntries → daemonClient.request())
vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    request: vi.fn().mockResolvedValue({ items: [], status: 'ready' }),
  },
}))

// Mock invokeWithTrace so it can be tracked (called at end of fetchClipboardItems thunk)
vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: vi.fn().mockResolvedValue({ items: [], status: 'ready' }),
}))

const mockGetClipboardEntry = vi.mocked(getClipboardEntry)
const mockGetEncryptionSessionStatus = vi.mocked(getEncryptionSessionStatus)
const mockInvokeWithTrace = vi.mocked(invokeWithTrace)

// Track dispatched actions
let dispatchedActions: Array<{ type: string; payload?: unknown }> = []

function createTestStore() {
  const store = configureStore({
    reducer: {
      clipboard: clipboardReducer,
    },
  })

  // Spy on dispatch to record actions
  const originalDispatch = store.dispatch.bind(store)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  store.dispatch = ((action: any) => {
    if (action && typeof action === 'object' && 'type' in action) {
      dispatchedActions.push(action as { type: string; payload?: unknown })
    }
    return originalDispatch(action)
  }) as typeof store.dispatch
  return store
}

function createWrapper() {
  const store = createTestStore()
  const Wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store, children })
  return { Wrapper, store }
}

describe('useClipboardEvents', () => {
  const mockUnlisten = vi.fn()

  beforeEach(() => {
    vi.clearAllMocks()
    dispatchedActions = []
    capturedClipboardHandler = null

    // Default: encryption not initialized (so ready = true per hook logic)
    mockGetEncryptionSessionStatus.mockResolvedValue({
      initialized: false,
      session_ready: false,
    })
  })

  it('registers clipboard listener on mount', async () => {
    const { Wrapper } = createWrapper()
    renderHook(() => useClipboardEvents(Filter.All), { wrapper: Wrapper })

    // Wait for the daemonWs.subscribe call to register
    await vi.waitFor(() => {
      expect(capturedClipboardHandler).not.toBeNull()
    })
  })

  it('P16-05: local origin event triggers getClipboardEntry and dispatches prependItem', async () => {
    const mockItem = {
      id: 'entry-1',
      is_downloaded: true,
      is_favorited: false,
      created_at: 1000,
      updated_at: 1000,
      active_time: 0,
      item: {
        text: { display_text: 'hello', has_detail: false, size: 5 },
        image: null,
        file: null,
        link: null,
        code: null,
        unknown: null,
      },
    }
    mockGetClipboardEntry.mockResolvedValue(mockItem)

    const { Wrapper } = createWrapper()
    renderHook(() => useClipboardEvents(Filter.All), { wrapper: Wrapper })

    // Wait for listener registration
    await vi.waitFor(() => {
      expect(capturedClipboardHandler).not.toBeNull()
    })

    // Wait for encryption status check to mark ready
    await act(async () => {
      await new Promise(r => setTimeout(r, 10))
    })

    // Simulate local clipboard event (new daemon WS format)
    await act(async () => {
      capturedClipboardHandler?.({
        topic: 'clipboard',
        eventType: 'clipboard.new-content',
        ts: 0,
        sessionId: null,
        payload: { entry_id: 'entry-1', preview: 'hello', origin: 'local' },
      })
      await new Promise(r => setTimeout(r, 10))
    })

    expect(mockGetClipboardEntry).toHaveBeenCalledWith('entry-1')

    const prependAction = dispatchedActions.find(a => a.type === 'clipboard/prependItem')
    expect(prependAction).toBeDefined()
    expect(prependAction?.payload).toEqual(mockItem)
  })

  it('P16-06: remote origin event triggers throttled full reload (fetchClipboardItems dispatch)', async () => {
    const { Wrapper } = createWrapper()
    renderHook(() => useClipboardEvents(Filter.All), { wrapper: Wrapper })

    // Wait for listener registration
    await vi.waitFor(() => {
      expect(capturedClipboardHandler).not.toBeNull()
    })

    // Wait for encryption ready
    await act(async () => {
      await new Promise(r => setTimeout(r, 10))
    })

    // Clear any previous invocations from the initial load
    mockInvokeWithTrace.mockClear()

    // Simulate remote clipboard event (new daemon WS format)
    await act(async () => {
      capturedClipboardHandler?.({
        topic: 'clipboard',
        eventType: 'clipboard.new-content',
        ts: 0,
        sessionId: null,
        payload: { entry_id: 'entry-2', preview: '...', origin: 'remote' },
      })
      // Wait for async thunk to dispatch and invoke
      await new Promise(r => setTimeout(r, 30))
    })

    // Remote event should trigger loadData which calls fetchClipboardItems -> invokeWithTrace('get_clipboard_entries', ...)
    expect(mockInvokeWithTrace).toHaveBeenCalledWith(
      'get_clipboard_entries',
      expect.objectContaining({ limit: 20, offset: 0 })
    )
    // getClipboardEntry should NOT have been called (that's for local events)
    expect(mockGetClipboardEntry).not.toHaveBeenCalled()
  })

  it('Deleted event dispatches removeItem', async () => {
    const { Wrapper } = createWrapper()
    renderHook(() => useClipboardEvents(Filter.All), { wrapper: Wrapper })

    // Wait for listener registration
    await vi.waitFor(() => {
      expect(capturedClipboardHandler).not.toBeNull()
    })

    await act(async () => {
      capturedClipboardHandler?.({
        topic: 'clipboard',
        eventType: 'clipboard.deleted',
        ts: 0,
        sessionId: null,
        payload: { entry_id: 'entry-del' },
      })
    })

    const removeAction = dispatchedActions.find(a => a.type === 'clipboard/removeItem')
    expect(removeAction).toBeDefined()
    expect(removeAction?.payload).toBe('entry-del')
  })

  it('cleans up listeners on unmount', async () => {
    const { Wrapper } = createWrapper()
    const { unmount } = renderHook(() => useClipboardEvents(Filter.All), { wrapper: Wrapper })

    await vi.waitFor(() => {
      expect(capturedClipboardHandler).not.toBeNull()
    })

    unmount()

    await vi.waitFor(() => {
      expect(capturedClipboardHandler).toBeNull()
    })
  })
})

import { listen } from '@tauri-apps/api/event'
import { renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useUINavigateListener } from '../useUINavigateListener'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

const mockListen = vi.mocked(listen)

describe('useUINavigateListener', () => {
  const mockUnlisten = vi.fn()

  beforeEach(() => {
    vi.clearAllMocks()
    mockListen.mockResolvedValue(mockUnlisten)
  })

  it('subscribes to ui://navigate on mount', () => {
    const onNavigate = vi.fn()
    renderHook(() => useUINavigateListener(onNavigate))

    expect(mockListen).toHaveBeenCalledWith('ui://navigate', expect.any(Function))
  })

  it('calls onNavigate for whitelisted /settings route', () => {
    const onNavigate = vi.fn()
    renderHook(() => useUINavigateListener(onNavigate))

    // Get the listener callback that was passed to listen()
    const listenerCallback = mockListen.mock.calls[0][1]

    // Simulate event with /settings payload
    listenerCallback({ payload: '/settings' } as never)

    expect(onNavigate).toHaveBeenCalledWith('/settings')
  })

  it('does not call onNavigate for non-whitelisted routes', () => {
    const onNavigate = vi.fn()
    const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    renderHook(() => useUINavigateListener(onNavigate))

    const listenerCallback = mockListen.mock.calls[0][1]

    // Simulate event with non-whitelisted route
    listenerCallback({ payload: '/admin' } as never)

    expect(onNavigate).not.toHaveBeenCalled()
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted route'))

    consoleSpy.mockRestore()
  })

  it('unlistens on unmount', async () => {
    const onNavigate = vi.fn()
    const { unmount } = renderHook(() => useUINavigateListener(onNavigate))

    unmount()

    // Wait for the promise to resolve
    await vi.waitFor(() => {
      expect(mockUnlisten).toHaveBeenCalled()
    })
  })
})

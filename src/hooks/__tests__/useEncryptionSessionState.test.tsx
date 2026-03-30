import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useEncryptionSessionState } from '../useEncryptionSessionState'
import { getEncryptionState } from '@/api/daemon'
import { getEncryptionSessionStatus as _getEncryptionSessionStatus } from '@/api/security'

// Mock daemonWs (hook now uses daemonWs.subscribe instead of Tauri listen)
let capturedEncryptionHandler: ((event: { eventType: string }) => void) | null = null
vi.mock('@/lib/daemon-ws', () => ({
  daemonWs: {
    subscribe: vi.fn((topics: string[], handler: (event: { eventType: string }) => void) => {
      if (topics.includes('encryption')) {
        capturedEncryptionHandler = handler
      }
      return () => {
        if (topics.includes('encryption')) {
          capturedEncryptionHandler = null
        }
      }
    }),
  },
}))

// Mock the daemon encryption module (used by useEncryptionSessionState — calls getEncryptionState from @/api/daemon)
vi.mock('@/api/daemon', () => ({
  getEncryptionState: vi.fn(),
}))

vi.mock('@/api/security', () => ({
  getEncryptionSessionStatus: vi.fn(),
}))

const mockGetEncryptionState = vi.mocked(getEncryptionState)

describe('useEncryptionSessionState', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    capturedEncryptionHandler = null
  })

  it('treats uninitialized encryption as ready', async () => {
    mockGetEncryptionState.mockResolvedValue({
      initialized: false,
      sessionReady: false,
    })

    const { result } = renderHook(() => useEncryptionSessionState())

    await waitFor(() => {
      expect(result.current.encryptionReady).toBe(true)
      expect(result.current.isLocked).toBe(false)
    })
  })

  it('treats initialized but locked encryption as locked', async () => {
    mockGetEncryptionState.mockResolvedValue({
      initialized: true,
      sessionReady: false,
    })

    const { result } = renderHook(() => useEncryptionSessionState())

    await waitFor(() => {
      expect(result.current.encryptionReady).toBe(false)
      expect(result.current.isLocked).toBe(true)
    })
  })

  it('switches to ready after encryption.session_ready event', async () => {
    mockGetEncryptionState.mockResolvedValue({
      initialized: true,
      sessionReady: false,
    })

    const { result } = renderHook(() => useEncryptionSessionState())

    await waitFor(() => {
      expect(capturedEncryptionHandler).not.toBeNull()
    })

    // Simulate encryption.session_ready from daemon WS
    capturedEncryptionHandler?.({ eventType: 'encryption.session_ready' })

    await waitFor(() => {
      expect(result.current.encryptionReady).toBe(true)
      expect(result.current.isLocked).toBe(false)
    })
  })
})

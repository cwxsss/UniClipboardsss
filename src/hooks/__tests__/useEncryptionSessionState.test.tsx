import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useEncryptionSessionState } from '../useEncryptionSessionState'
import { getEncryptionSessionStatus } from '@/api/security'

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

vi.mock('@/api/security', () => ({
  getEncryptionSessionStatus: vi.fn(),
}))

const mockGetEncryptionSessionStatus = vi.mocked(getEncryptionSessionStatus)

describe('useEncryptionSessionState', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    capturedEncryptionHandler = null
  })

  it('treats uninitialized encryption as ready', async () => {
    mockGetEncryptionSessionStatus.mockResolvedValue({
      initialized: false,
      session_ready: false,
    })

    const { result } = renderHook(() => useEncryptionSessionState())

    await waitFor(() => {
      expect(result.current.encryptionReady).toBe(true)
      expect(result.current.isLocked).toBe(false)
    })
  })

  it('treats initialized but locked encryption as locked', async () => {
    mockGetEncryptionSessionStatus.mockResolvedValue({
      initialized: true,
      session_ready: false,
    })

    const { result } = renderHook(() => useEncryptionSessionState())

    await waitFor(() => {
      expect(result.current.encryptionReady).toBe(false)
      expect(result.current.isLocked).toBe(true)
    })
  })

  it('switches to ready after encryption.sessionReady event', async () => {
    mockGetEncryptionSessionStatus.mockResolvedValue({
      initialized: true,
      session_ready: false,
    })

    const { result } = renderHook(() => useEncryptionSessionState())

    await waitFor(() => {
      expect(capturedEncryptionHandler).not.toBeNull()
    })

    // Simulate encryption.sessionReady from daemon WS
    capturedEncryptionHandler?.({ eventType: 'encryption.sessionReady' })

    await waitFor(() => {
      expect(result.current.encryptionReady).toBe(true)
      expect(result.current.isLocked).toBe(false)
    })
  })
})

/**
 * Integration tests for DaemonClient encryption API module.
 *
 * Covers:
 * - GET /encryption/state — correct state shapes (initialized/sessionReady)
 * - POST /encryption/unlock — auto-unlock (keyring-based, no passphrase)
 * - POST /encryption/lock — success
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it, beforeEach, afterEach } from 'vitest'
import {
  setupFetchMock,
  teardownFetchMock,
  makeEncryptionStateDto,
  mockResponse,
  mockErrorResponse,
} from './_test-helpers'
import { getEncryptionState, unlockEncryption, lockEncryption } from '@/api/daemon/encryption'
import { DaemonErrorCode } from '@/api/daemon/errors'

describe('Encryption API', () => {
  let mockFetch: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    const { mockFetch: mf } = setupFetchMock()
    mockFetch = mf
  })

  afterEach(() => {
    teardownFetchMock()
  })

  // ── GET /encryption/state ──────────────────────────────────

  describe('getEncryptionState()', () => {
    it('returns initialized:true, sessionReady:false when passphrase set but locked', async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse({ data: { initialized: true, sessionReady: false }, ts: Date.now() })
      )

      const state = await getEncryptionState()

      expect(state.initialized).toBe(true)
      expect(state.sessionReady).toBe(false)
      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/encryption/state')
    })

    it('returns initialized:false, sessionReady:false when passphrase not configured', async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse({ data: { initialized: false, sessionReady: false }, ts: Date.now() })
      )

      const state = await getEncryptionState()

      expect(state.initialized).toBe(false)
      expect(state.sessionReady).toBe(false)
    })

    it('returns initialized:true, sessionReady:true when unlocked', async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse({ data: { initialized: true, sessionReady: true }, ts: Date.now() })
      )

      const state = await getEncryptionState()

      expect(state.initialized).toBe(true)
      expect(state.sessionReady).toBe(true)
    })

    it('wraps response in data envelope with ts', async () => {
      const ts = 1710000000000
      mockFetch.mockResolvedValueOnce(mockResponse({ data: makeEncryptionStateDto(), ts }))

      const state = await getEncryptionState()

      expect(state.initialized).toBe(true)
      // ts is not exposed in the response — only the data envelope is unwrapped
    })

    it('re-throws DaemonApiError on HTTP failure', async () => {
      mockFetch.mockResolvedValueOnce(mockErrorResponse(500, { error: '500 on /encryption/state' }))

      await expect(getEncryptionState()).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
      })
    })
  })

  // ── POST /encryption/unlock ─────────────────────────────────

  describe('unlockEncryption()', () => {
    it('sends POST to /encryption/unlock with no body and resolves on success', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ data: { success: true }, ts: Date.now() }))

      await expect(unlockEncryption()).resolves.toBeUndefined()

      const [url, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(url).toContain('/encryption/unlock')
      expect((opts as { method: string }).method).toBe('POST')
      // No body sent for auto-unlock
      expect((opts as { body?: string }).body).toBeUndefined()
    })

    it('resolves successfully when encryption not initialized (success: false in body)', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ data: { success: false }, ts: Date.now() }))

      // Even success=false still resolves (no passphrase needed, nothing to do)
      await expect(unlockEncryption()).resolves.toBeUndefined()
    })

    it('re-throws DaemonApiError on auto-unlock failure (500)', async () => {
      mockFetch.mockResolvedValueOnce(mockErrorResponse(500, { error: 'keyring access denied' }))

      await expect(unlockEncryption()).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
        message: 'keyring access denied',
      })
    })
  })

  // ── POST /encryption/lock ───────────────────────────────────

  describe('lockEncryption()', () => {
    it('sends POST to /encryption/lock and resolves on success', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ data: { success: true }, ts: Date.now() }))

      await expect(lockEncryption()).resolves.toBeUndefined()

      const [url, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(url).toContain('/encryption/lock')
      expect((opts as { method: string }).method).toBe('POST')
    })

    it('re-throws DaemonApiError on HTTP failure', async () => {
      mockFetch.mockResolvedValueOnce(mockErrorResponse(500, { error: '500 on /encryption/lock' }))

      await expect(lockEncryption()).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
      })
    })
  })
})

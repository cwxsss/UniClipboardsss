/**
 * Integration tests for DaemonClient encryption API module.
 *
 * Covers:
 * - GET /encryption/state — correct state shapes (initialized/sessionReady)
 * - POST /encryption/unlock — wrong passphrase (401), not initialized (400), success
 * - POST /encryption/lock — success
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it, beforeEach, afterEach } from 'vitest'
import {
  getEncryptionState,
  unlockEncryption,
  lockEncryption,
} from '@/api/daemon/encryption'
import { DaemonErrorCode } from '@/api/daemon/errors'
import {
  setupFetchMock,
  teardownFetchMock,
  makeEncryptionStateDto,
  mockResponse,
  mockErrorResponse,
} from './_test-helpers'

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
        mockResponse({ data: { initialized: true, sessionReady: false }, ts: Date.now() }),
      )

      const state = await getEncryptionState()

      expect(state.initialized).toBe(true)
      expect(state.sessionReady).toBe(false)
      const [url] = mockFetch.mock.calls[0] as [string]
      expect(url).toContain('/encryption/state')
    })

    it('returns initialized:false, sessionReady:false when passphrase not configured', async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse({ data: { initialized: false, sessionReady: false }, ts: Date.now() }),
      )

      const state = await getEncryptionState()

      expect(state.initialized).toBe(false)
      expect(state.sessionReady).toBe(false)
    })

    it('returns initialized:true, sessionReady:true when unlocked', async () => {
      mockFetch.mockResolvedValueOnce(
        mockResponse({ data: { initialized: true, sessionReady: true }, ts: Date.now() }),
      )

      const state = await getEncryptionState()

      expect(state.initialized).toBe(true)
      expect(state.sessionReady).toBe(true)
    })

    it('wraps response in data envelope with ts', async () => {
      const ts = 1710000000000
      mockFetch.mockResolvedValueOnce(
        mockResponse({ data: makeEncryptionStateDto(), ts }),
      )

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

  describe('unlockEncryption(passphrase)', () => {
    it('sends POST with passphrase in body and resolves on success', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ data: { success: true }, ts: Date.now() }))

      await expect(unlockEncryption('correct-passphrase')).resolves.toBeUndefined()

      const [url, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(url).toContain('/encryption/unlock')
      expect((opts as { method: string }).method).toBe('POST')
      expect(JSON.parse((opts as { body: string }).body)).toEqual({ passphrase: 'correct-passphrase' })
    })

    it('returns 401 with UNAUTHORIZED code on wrong passphrase', async () => {
      // The DaemonClient auto-retries once on 401 by refreshing the session.
      // Sequence: unlock(401) → refreshSession → unlock-retry(401) → rejected.
      // mockImplementation gives us full control over successive calls.
      mockFetch.mockImplementation(async (input) => {
        const url = typeof input === 'string' ? input : (input as URL).toString()
        if (url.includes('/encryption/unlock')) {
          return mockErrorResponse(401, { error: 'Invalid passphrase' })
        }
        if (url.includes('/auth/connect')) {
          // refreshSession success
          return mockResponse({ sessionToken: 'fresh-token', expiresInSecs: 300, refreshAtSecs: 240 })
        }
        return mockErrorResponse(500, { error: 'unexpected call' })
      })

      await expect(unlockEncryption('wrong-passphrase')).rejects.toMatchObject({
        code: DaemonErrorCode.UNAUTHORIZED,
        message: 'Invalid passphrase',
      })
    })

    it('returns 400 / INTERNAL_ERROR when encryption not initialized', async () => {
      mockFetch.mockResolvedValueOnce(
        mockErrorResponse(400, { error: 'Encryption not initialized' }),
      )

      await expect(unlockEncryption('any-passphrase')).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
        message: 'Encryption not initialized',
      })
    })

    it('passes through passphrase with whitespace (trimming is caller responsibility)', async () => {
      mockFetch.mockResolvedValueOnce(mockResponse({ data: { success: true }, ts: Date.now() }))

      await unlockEncryption('  leading-trailing-spaces  ')

      const [, opts] = mockFetch.mock.calls[0] as [string, RequestInit]
      expect(JSON.parse((opts as { body: string }).body)).toEqual({
        passphrase: '  leading-trailing-spaces  ',
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

/**
 * Integration tests for daemon-auth module (loadDaemonAuth, verifyAuthState, waitForEncryptionReady).
 *
 * Covers:
 * - loadDaemonAuth(): Tauri event → DaemonClient init → session token obtained, stored in memory
 * - verifyAuthState(): daemon health + encryption state checking
 * - waitForEncryptionReady(): timeout and session-ready polling
 * - Token stored in memory, not localStorage/sessionStorage/cookies
 * - Bearer token never appears in console
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest'
import type { SessionToken } from '@/api/daemon/types'

// ── Tauri event mock (hoisted — shares closure with emitTauriEvent) ──

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const _pendingTauriListeners: Map<string, (payload: unknown) => void> = new Map()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((eventName: string, handler: (event: { payload: unknown }) => void) => {
    _pendingTauriListeners.set(eventName, handler as (payload: unknown) => void)
    return Promise.resolve(() => _pendingTauriListeners.delete(eventName))
  }),
}))

function emitTauriEvent<T>(eventName: string, payload: T): void {
  const handler = _pendingTauriListeners.get(eventName)
  if (handler) { handler({ payload } as { payload: unknown }); _pendingTauriListeners.delete(eventName) }
}

// ── Mock daemonClient (module-level, controlled queue) ────────

let _session: SessionToken | null = null
let _fetchQueue: Response[] = []

const TEST_PAYLOAD = {
  baseUrl: 'http://127.0.0.1:42715',
  wsUrl: 'ws://127.0.0.1:42715/ws',
  token: 'tauri-bearer-token',
}

function reset(): void { _session = null; _fetchQueue = [] }

vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    get initialized() { return true },
    get wsUrl() { return TEST_PAYLOAD.wsUrl },
    get currentSession() { return _session },
    initialize(_config: unknown) { /* no-op */ },
    async refreshSession() {
      const r = _fetchQueue.shift() ?? authResponse('mock-jwt-session')
      if (!r.ok) throw new Error(`auth failed ${r.status}`)
      const data = await r.json() as { sessionToken: string; expiresInSecs: number }
      _session = { token: data.sessionToken, expiresAt: Date.now() + data.expiresInSecs * 1000, encryptionReady: false }
      return _session
    },
    async request<T>(endpoint: string, _opts = {}): Promise<T> {
      const r = _fetchQueue.shift() ?? okResponse({})
      if (!r.ok) throw Object.assign(new Error(`${r.status} on ${endpoint}`), { code: 'INTERNAL_ERROR' })
      return r.json() as T
    },
    destroy() { _session = null; _fetchQueue = [] },
  },
}))

function authResponse(sessionToken = 'mock-jwt-session'): Response {
  return new Response(JSON.stringify({ sessionToken, expiresInSecs: 300, refreshAtSecs: 240 }), {
    status: 200, headers: { 'Content-Type': 'application/json' },
  })
}

function okResponse(body: object): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
}

function errResponse(status: number, body?: unknown): Response {
  return new Response(JSON.stringify(body ?? { error: `HTTP ${status}` }), {
    status, headers: { 'Content-Type': 'application/json' },
  })
}

// ── Tests ────────────────────────────────────────────────────

describe('daemon-auth module', () => {
  beforeEach(() => { _pendingTauriListeners.clear(); reset() })
  afterEach(() => { _pendingTauriListeners.clear(); reset() })

  // ── loadDaemonAuth() ────────────────────────────────────────

  describe('loadDaemonAuth()', () => {
    it('initializes DaemonClient with connection info from Tauri event', async () => {
      const { loadDaemonAuth } = await import('@/lib/daemon-auth')
      _fetchQueue.push(authResponse())

      const p = loadDaemonAuth()
      emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)
      const result = await p

      expect(result.session).not.toBeNull()
      expect(result.session.token).toBe('mock-jwt-session')
      expect(result.wsUrl).toBe('ws://127.0.0.1:42715/ws')
    })

    it('session token is stored in daemonClient.currentSession, not external storage', async () => {
      const { loadDaemonAuth } = await import('@/lib/daemon-auth')
      _fetchQueue.push(authResponse())

      const p = loadDaemonAuth()
      emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)
      await p

      expect(_session?.token).toBe('mock-jwt-session')
      expect(localStorage.getItem('sessionToken')).toBeNull()
      expect(sessionStorage.getItem('sessionToken')).toBeNull()
      expect(document.cookie).not.toContain('jwt-session')
    })

    it('returns wsUrl matching the Tauri event payload', async () => {
      const { loadDaemonAuth } = await import('@/lib/daemon-auth')
      _fetchQueue.push(authResponse())

      const p = loadDaemonAuth()
      emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)
      const result = await p

      expect(result.wsUrl).toBe(TEST_PAYLOAD.wsUrl)
    })

    it('promise stays pending when Tauri event never fires', async () => {
      vi.useFakeTimers()
      const { loadDaemonAuth } = await import('@/lib/daemon-auth')

      const p = loadDaemonAuth()
      vi.advanceTimersByTime(1000)
      let settled = false
      void p.then(() => { settled = true })
      vi.advanceTimersByTime(5000)
      expect(settled).toBe(false)
      vi.useRealTimers()
    })

    it('bearer token never appears in console.error or console.warn', async () => {
      const { loadDaemonAuth } = await import('@/lib/daemon-auth')
      const errorSpy = vi.spyOn(console, 'error').mockReturnValue(undefined)
      const warnSpy = vi.spyOn(console, 'warn').mockReturnValue(undefined)
      _fetchQueue.push(authResponse())

      const p = loadDaemonAuth()
      emitTauriEvent('daemon://connection-info', TEST_PAYLOAD)
      await p

      const leaks = (spy: ReturnType<typeof vi.spyOn>) =>
        spy.mock.calls.filter(a => a.some((x: unknown) => typeof x === 'string' && x.includes('tauri-bearer-token')))
      expect(leaks(errorSpy)).toHaveLength(0)
      expect(leaks(warnSpy)).toHaveLength(0)
    })
  })

  // ── verifyAuthState() ───────────────────────────────────────

  describe('verifyAuthState()', () => {
    it('returns daemonReady=true when GET /health returns ok', async () => {
      const { verifyAuthState } = await import('@/lib/daemon-auth')
      _fetchQueue.push(okResponse({ status: 'ok' }))
      _fetchQueue.push(errResponse(401)) // encryption state 401

      const result = await verifyAuthState()
      expect(result.daemonReady).toBe(true)
    })

    it('returns daemonReady=false when GET /health fails', async () => {
      const { verifyAuthState } = await import('@/lib/daemon-auth')
      _fetchQueue.push(errResponse(500))

      const result = await verifyAuthState()
      expect(result.daemonReady).toBe(false)
    })

    it('sets encryptionInitialized=true when GET /encryption/state returns initialized:true', async () => {
      const { verifyAuthState } = await import('@/lib/daemon-auth')
      _fetchQueue.push(okResponse({ status: 'ok' }))
      _fetchQueue.push(okResponse({ data: { initialized: true, sessionReady: false }, ts: 1710000000000 }))

      const result = await verifyAuthState()
      expect(result.daemonReady).toBe(true)
      expect(result.encryptionInitialized).toBe(true)
      expect(result.encryptionSessionReady).toBe(false)
    })

    it('sets encryptionSessionReady=true when GET /encryption/state returns sessionReady:true', async () => {
      const { verifyAuthState } = await import('@/lib/daemon-auth')
      _fetchQueue.push(okResponse({ status: 'ok' }))
      _fetchQueue.push(okResponse({ data: { initialized: true, sessionReady: true }, ts: 1710000000000 }))

      const result = await verifyAuthState()
      expect(result.encryptionSessionReady).toBe(true)
    })

    it('returns early with daemonReady=false when daemon unreachable', async () => {
      const { verifyAuthState } = await import('@/lib/daemon-auth')
      _fetchQueue.push(errResponse(500))

      const result = await verifyAuthState()
      expect(result.daemonReady).toBe(false)
      expect(result.encryptionInitialized).toBe(false)
      expect(result.encryptionSessionReady).toBe(false)
    })

    it('daemonReady stays true even when encryption state check fails with 401', async () => {
      const { verifyAuthState } = await import('@/lib/daemon-auth')
      _fetchQueue.push(okResponse({ status: 'ok' }))
      _fetchQueue.push(errResponse(401))

      const result = await verifyAuthState()
      expect(result.daemonReady).toBe(true)
    })
  })

  // ── waitForEncryptionReady() ─────────────────────────────

  describe('waitForEncryptionReady()', () => {
    it('returns true when encryption state reaches sessionReady=true', async () => {
      const { waitForEncryptionReady } = await import('@/lib/daemon-auth')
      _fetchQueue.push(okResponse({ data: { initialized: true, sessionReady: true }, ts: 1710000000000 }))

      const result = await waitForEncryptionReady(5000)
      expect(result).toBe(true)
    })

    it('returns false when timeout expires before sessionReady=true', async () => {
      const { waitForEncryptionReady } = await import('@/lib/daemon-auth')
      _fetchQueue.push(okResponse({ data: { initialized: true, sessionReady: false }, ts: 1710000000000 }))

      vi.useFakeTimers()
      const p = waitForEncryptionReady(3000)
      await vi.runAllTimersAsync()
      const result = await p
      vi.useRealTimers()

      expect(result).toBe(false)
    })

    it('continues polling through transient errors until deadline', async () => {
      const { waitForEncryptionReady } = await import('@/lib/daemon-auth')
      _fetchQueue.push(errResponse(500))
      _fetchQueue.push(errResponse(503))
      _fetchQueue.push(okResponse({ data: { initialized: true, sessionReady: true }, ts: 1710000000000 }))

      const result = await waitForEncryptionReady(5000)
      expect(result).toBe(true)
      expect(_fetchQueue.length).toBe(0) // all consumed
    })

    it('does not crash when polling encounters network errors', async () => {
      const { waitForEncryptionReady } = await import('@/lib/daemon-auth')
      _fetchQueue.push(errResponse(500))

      vi.useFakeTimers()
      const p = waitForEncryptionReady(2000)
      await vi.runAllTimersAsync()
      const result = await p
      vi.useRealTimers()

      expect(result).toBe(false)
    })
  })
})

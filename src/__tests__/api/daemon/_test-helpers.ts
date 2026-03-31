/**
 * Shared test helpers for DaemonClient API integration tests.
 *
 * Provides a shared mock for daemonClient.request that all daemon API tests use.
 * The vi.mock for @/api/daemon/client is defined HERE (in _test-helpers.ts) so
 * that only ONE mock is applied for this module across all test files.
 *
 * Usage in each test file:
 *   import { mockDaemonClient, setupMockClient, teardownMockClient } from './_test-helpers'
 *
 *   beforeEach(() => { mockDaemonClient.request.mockReset() })
 *   afterEach(() => { teardownMockClient() })
 */

import { vi } from 'vitest'
import type { ClipboardEntryDto } from '@/api/daemon/clipboard'
import type { EncryptionStateResponse } from '@/api/daemon/encryption'
import { DaemonErrorCode, DaemonApiError } from '@/api/daemon/errors'
import type { DaemonApiError as DaemonApiErrorType } from '@/api/daemon/errors'
import type { Settings } from '@/api/daemon/settings'

// ── Mock daemonClient ──────────────────────────────────────────
// MUST be at the top of this file so vi.mock can reference it before hoisting.
const mockRequest = vi.fn()
const mockRefreshSession = vi.fn()

export const mockDaemonClient = {
  initialize: vi.fn(),
  destroy: vi.fn(),
  request: mockRequest,
  refreshSession: mockRefreshSession,
  session: null as { token: string; expiresAt: number; encryptionReady: boolean } | null,
  get initialized() {
    return true
  },
  get wsUrl() {
    return null
  },
  get currentSession() {
    return null
  },
}

// Hoisted mock — this runs before any import of @/api/daemon/client
vi.mock('@/api/daemon/client', () => ({
  daemonClient: mockDaemonClient,
}))

// ── Helper functions ───────────────────────────────────────────

/**
 * Reset mockDaemonClient.request between tests.
 * Call this in beforeEach.
 */
export function setupMockClient(): void {
  mockDaemonClient.request.mockReset()
  mockDaemonClient.request.mockResolvedValue(undefined)
  mockDaemonClient.initialize.mockReset()
  mockDaemonClient.destroy.mockReset()
}

/**
 * Restore all mocks.
 * Call this in afterEach.
 */
export function teardownMockClient(): void {
  vi.restoreAllMocks()
}

// ── Legacy fetch-based helpers (for pre-existing tests) ───────────
// These are kept for backward compatibility with pre-existing tests that
// used vi.spyOn(globalThis, 'fetch') via setupFetchMock/teardownFetchMock.

/**
 * @deprecated Use setupMockClient + mockDaemonClient.request.mockResolvedValueOnce instead.
 */
export function setupFetchMock(): { mockFetch: ReturnType<typeof vi.spyOn> } {
  const mockFetch = vi.spyOn(globalThis, 'fetch')
  return { mockFetch }
}

/**
 * @deprecated Use teardownMockClient instead.
 */
export function teardownFetchMock(): void {
  vi.restoreAllMocks()
}

// ── Mock fetch response builders ──────────────────────────────

/** Build a Response that returns the given JSON payload and status. */
export function mockResponse<T>(payload: T, status = 200): Response {
  return new Response(JSON.stringify(payload), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

/** Build a Response representing a daemon error (non-ok status). */
export function mockErrorResponse(status: number, body?: unknown): Response {
  return new Response(JSON.stringify(body ?? { error: `HTTP ${status}` }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

// ── Error factories ────────────────────────────────────────────

export function makeNotFoundError(
  message = '404 on /clipboard/entries/test-id'
): DaemonApiErrorType {
  return new DaemonApiError(DaemonErrorCode.NOT_FOUND, message)
}

export function makeUnauthorizedError(message = '401 Unauthorized'): DaemonApiErrorType {
  return new DaemonApiError(DaemonErrorCode.UNAUTHORIZED, message)
}

export function makeValidationError(
  message = 'validation failed',
  details?: unknown
): DaemonApiErrorType {
  return new DaemonApiError(DaemonErrorCode.INTERNAL_ERROR, message, details)
}

// ── Mock data factories ────────────────────────────────────────

export function makeEntryDto(overrides: Partial<ClipboardEntryDto> = {}): ClipboardEntryDto {
  return {
    id: 'entry-1',
    preview: 'Hello, world!',
    has_detail: false,
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
    ...overrides,
  }
}

export function makeSettingsDto(overrides: Partial<Settings> = {}): Settings {
  return {
    schema_version: 1,
    general: {
      auto_start: false,
      silent_start: false,
      auto_check_update: true,
      theme: 'system',
      theme_color: null,
      language: null,
      device_name: 'Test Device',
      update_channel: null,
    },
    sync: {
      auto_sync: true,
      sync_frequency: 'realtime',
      content_types: {
        text: true,
        image: true,
        link: true,
        file: true,
        code_snippet: true,
        rich_text: true,
      },
      max_file_size_mb: 50,
    },
    retention_policy: {
      enabled: false,
      rules: [],
      skip_pinned: true,
      evaluation: 'any_match',
    },
    security: {
      encryption_enabled: true,
      passphrase_configured: false,
      auto_unlock_enabled: false,
    },
    pairing: {
      step_timeout: 30,
      user_verification_timeout: 60,
      session_timeout: 3600,
      max_retries: 3,
      protocol_version: '1.0.0',
    },
    keyboard_shortcuts: {},
    file_sync: {
      file_sync_enabled: true,
      small_file_threshold: 1024,
      max_file_size: 1024 * 1024 * 100,
      file_cache_quota_per_device: 1024 * 1024 * 500,
      file_retention_hours: 168,
      file_auto_cleanup: true,
    },
    ...overrides,
  }
}

export function makeEncryptionStateDto(
  overrides: Partial<EncryptionStateResponse> = {}
): EncryptionStateResponse {
  return {
    initialized: true,
    sessionReady: false,
    ...overrides,
  }
}

export function makeStorageStatsDto(
  overrides: Partial<{
    total_entries: number
    total_size_bytes: number
    cache_size_bytes: number
    oldest_entry_ts: number | null
    newest_entry_ts: number | null
  }> = {}
): {
  total_entries: number
  total_size_bytes: number
  cache_size_bytes: number
  oldest_entry_ts: number | null
  newest_entry_ts: number | null
} {
  return {
    total_entries: 42,
    total_size_bytes: 1024 * 512,
    cache_size_bytes: 1024 * 128,
    oldest_entry_ts: 1709000000000,
    newest_entry_ts: 1710000000000,
    ...overrides,
  }
}

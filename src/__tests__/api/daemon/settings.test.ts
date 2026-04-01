/**
 * Integration tests for DaemonClient settings API module.
 *
 * Covers:
 * - GET /settings — correct shape, snake_case field names
 * - PUT /settings — validation errors (400), partial update, full success
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it, beforeEach, afterEach } from 'vitest'
import {
  makeSettingsDto,
  setupMockClient,
  teardownMockClient,
  mockDaemonClient,
  makeValidationError,
  makeNotFoundError,
} from './_test-helpers'
import { DaemonErrorCode } from '@/api/daemon/errors'
import { getSettings, updateSettings } from '@/api/daemon/settings'
import type { Settings } from '@/api/daemon/settings'

describe('Settings API', () => {
  beforeEach(() => {
    setupMockClient()
  })

  afterEach(() => {
    teardownMockClient()
  })

  // ── GET /settings ───────────────────────────────────────────

  describe('getSettings()', () => {
    it('returns the full Settings object on success', async () => {
      const settings = makeSettingsDto()
      mockDaemonClient.request.mockResolvedValueOnce({ data: settings, ts: Date.now() })

      const result = await getSettings()

      expect(result).toHaveProperty('schema_version')
      expect(result).toHaveProperty('general')
      expect(result).toHaveProperty('sync')
      expect(result).toHaveProperty('retention_policy')
      expect(result).toHaveProperty('security')
      expect(result).toHaveProperty('pairing')
      expect(result).toHaveProperty('keyboard_shortcuts')
      expect(result).toHaveProperty('file_sync')
    })

    it('general settings use snake_case field names matching daemon serde', async () => {
      const settings = makeSettingsDto({
        general: {
          auto_start: true,
          silent_start: false,
          auto_check_update: false,
          theme: 'dark',
          theme_color: '#1a1a1a',
          language: 'en-US',
          device_name: 'My MacBook',
          update_channel: 'stable',
        },
      })
      mockDaemonClient.request.mockResolvedValueOnce({ data: settings, ts: Date.now() })

      const result = await getSettings()

      // snake_case field names from daemon default serde serialisation
      expect(result.general).toHaveProperty('auto_start')
      expect(result.general).toHaveProperty('auto_check_update')
      expect(result.general).toHaveProperty('theme_color')
      expect(result.general).toHaveProperty('device_name')
      expect(result.general).toHaveProperty('update_channel')
      expect(result.general.theme).toBe('dark')
    })

    it('sync settings expose all content type toggles', async () => {
      const settings = makeSettingsDto({
        sync: {
          auto_sync: true,
          sync_frequency: 'interval',
          content_types: {
            text: true,
            image: false,
            link: true,
            file: false,
            code_snippet: true,
            rich_text: false,
          },
          max_file_size_mb: 25,
        },
      })
      mockDaemonClient.request.mockResolvedValueOnce({ data: settings, ts: Date.now() })

      const result = await getSettings()

      expect(result.sync.content_types.text).toBe(true)
      expect(result.sync.content_types.image).toBe(false)
      expect(result.sync.content_types.code_snippet).toBe(true)
    })

    it('retention policy contains rules array with discriminated union types', async () => {
      const settings = makeSettingsDto({
        retention_policy: {
          enabled: true,
          rules: [{ by_age: { max_age: 86400 * 30 } }, { by_count: { max_items: 500 } }],
          skip_pinned: true,
          evaluation: 'all_match',
        },
      })
      mockDaemonClient.request.mockResolvedValueOnce({ data: settings, ts: Date.now() })

      const result = await getSettings()

      expect(result.retention_policy.enabled).toBe(true)
      expect(result.retention_policy.rules).toHaveLength(2)
      expect(result.retention_policy.rules[0]).toHaveProperty('by_age')
      expect(result.retention_policy.rules[1]).toHaveProperty('by_count')
    })

    it('re-throws DaemonApiError on HTTP failure', async () => {
      mockDaemonClient.request.mockRejectedValueOnce(makeNotFoundError('500 on /settings'))

      await expect(getSettings()).rejects.toMatchObject({
        code: DaemonErrorCode.NOT_FOUND,
      })
    })
  })

  // ── PUT /settings ───────────────────────────────────────────

  describe('updateSettings(partial)', () => {
    it('sends PUT with snake_case payload matching Settings schema', async () => {
      mockDaemonClient.request.mockResolvedValueOnce({ data: { success: true }, ts: Date.now() })

      await updateSettings({
        schema_version: 1,
        general: {
          auto_start: true,
          silent_start: false,
          auto_check_update: true,
          theme: 'light',
          theme_color: null,
          language: null,
          device_name: 'New Name',
          update_channel: null,
        },
      })

      expect(mockDaemonClient.request).toHaveBeenCalledTimes(1)
      const [, opts] = mockDaemonClient.request.mock.calls[0] as [string, RequestInit]
      expect((opts as { method: string }).method).toBe('PUT')
      const body = (opts as { body: Record<string, unknown> }).body
      expect(body).toHaveProperty('general')
      expect(body.general as Record<string, unknown>).toHaveProperty('auto_start')
      expect(body.general as Record<string, unknown>).toHaveProperty('theme')
    })

    it('accepts a minimal partial update with only changed fields', async () => {
      mockDaemonClient.request.mockResolvedValueOnce({ data: { success: true }, ts: Date.now() })

      await updateSettings({
        general: {
          auto_start: false,
          silent_start: false,
          auto_check_update: true,
          theme: 'dark',
          theme_color: null,
          language: null,
          device_name: null,
          update_channel: null,
        },
      })

      expect(mockDaemonClient.request).toHaveBeenCalledTimes(1)
    })

    it('encodes keyboard_shortcuts as a patch object with nested shortcuts map', async () => {
      mockDaemonClient.request.mockResolvedValueOnce({ data: { success: true }, ts: Date.now() })

      await updateSettings({
        keyboard_shortcuts: {
          toggle_main_window: ['CommandOrControl+Shift+V', 'Alt+Shift+V'],
        },
      })

      expect(mockDaemonClient.request).toHaveBeenCalledTimes(1)
      const [, opts] = mockDaemonClient.request.mock.calls[0] as [string, RequestInit]
      const body = (opts as { body: Record<string, unknown> }).body
      expect(body.keyboard_shortcuts).toEqual({
        shortcuts: {
          toggle_main_window: ['CommandOrControl+Shift+V', 'Alt+Shift+V'],
        },
      })
    })

    it('re-throws DaemonApiError with validation detail on 400', async () => {
      mockDaemonClient.request.mockRejectedValueOnce(
        makeValidationError('field "theme" must be one of light|dark|system', {
          field: 'general.theme',
          constraint: 'enum',
        })
      )

      await expect(
        updateSettings({
          general: {
            auto_start: false,
            silent_start: false,
            auto_check_update: true,
            theme: 'invalid-theme' as Settings['general']['theme'],
            theme_color: null,
            language: null,
            device_name: null,
            update_channel: null,
          },
        })
      ).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
      })
    })

    it('re-throws DaemonApiError on HTTP failure', async () => {
      mockDaemonClient.request.mockRejectedValueOnce(makeValidationError('500 on /settings'))

      await expect(updateSettings({})).rejects.toMatchObject({
        code: DaemonErrorCode.INTERNAL_ERROR,
      })
    })
  })
})

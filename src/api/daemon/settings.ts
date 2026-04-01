/**
 * Settings API module — typed accessors for daemon settings endpoints.
 *
 * 设置 API 模块 — daemon 设置端点的类型化访问器。
 *
 * # Endpoints / 端点
 * - `GET /settings` → current application settings
 * - `PUT /settings` → partial-update (deep merge on server side)
 *
 * # Note / 注意
 * Unlike the Tauri command, the daemon HTTP settings endpoint does NOT apply
 * OS-level side effects (autostart registration, keyboard shortcut updates).
 * It only persists the settings domain model.
 */

import { daemonClient } from './client'

// ── Enums ──────────────────────────────────────────────────────

/** Theme preference. / 主题偏好。 */
export type Theme = 'light' | 'dark' | 'system'

/** Update channel override. / 更新通道覆盖。 */
export type UpdateChannel = 'stable' | 'alpha' | 'beta' | 'rc'

/** Sync frequency mode. / 同步频率模式。 */
export type SyncFrequency = 'realtime' | 'interval'

/** Retention rule evaluation strategy. / 保留规则评估策略。 */
export type RuleEvaluation = 'any_match' | 'all_match'

// ── Sub-setting interfaces ─────────────────────────────────────

/** General application settings. / 常规应用设置。 */
export interface GeneralSettings {
  auto_start: boolean
  silent_start: boolean
  auto_check_update: boolean
  theme: Theme
  theme_color: string | null
  language: string | null
  device_name: string | null
  update_channel?: UpdateChannel | null
}

/** Content type toggles for sync filtering. / 同步过滤的内容类型开关。 */
export interface ContentTypes {
  text: boolean
  image: boolean
  link: boolean
  file: boolean
  code_snippet: boolean
  rich_text: boolean
}

/** Sync behaviour settings. / 同步行为设置。 */
export interface SyncSettings {
  auto_sync: boolean
  sync_frequency: SyncFrequency
  content_types: ContentTypes
  max_file_size_mb: number
}

/** Security / encryption settings. / 安全/加密设置。 */
export interface SecuritySettings {
  encryption_enabled: boolean
  passphrase_configured: boolean
  auto_unlock_enabled: boolean
}

/** Pairing timeout and protocol settings. / 配对超时和协议设置。 */
export interface PairingSettings {
  /** Step timeout in seconds. */
  step_timeout: number
  /** User verification timeout in seconds. */
  user_verification_timeout: number
  /** Session timeout in seconds. */
  session_timeout: number
  max_retries: number
  protocol_version: string
}

/** File sync settings. / 文件同步设置。 */
export interface FileSyncSettings {
  file_sync_enabled: boolean
  small_file_threshold: number
  max_file_size: number
  file_cache_quota_per_device: number
  file_retention_hours: number
  file_auto_cleanup: boolean
}

/**
 * Retention rule — discriminated union matching the Rust `RetentionRule` enum.
 *
 * 保留规则 — 与 Rust `RetentionRule` 枚举匹配的可区分联合类型。
 */
export type RetentionRule =
  | { by_age: { max_age: number } }
  | { by_count: { max_items: number } }
  | { by_content_type: { content_type: ContentTypes; max_age: number } }
  | { by_total_size: { max_bytes: number } }
  | { sensitive: { max_age: number } }

/** Retention policy configuration. / 保留策略配置。 */
export interface RetentionPolicy {
  enabled: boolean
  rules: RetentionRule[]
  skip_pinned: boolean
  evaluation: RuleEvaluation
}

/**
 * Keyboard shortcut value — single key combo or multiple alternatives.
 *
 * 键盘快捷键值 — 单个按键组合或多个备选方案。
 */
export type ShortcutKey = string | string[]

// ── Top-level Settings ─────────────────────────────────────────

/**
 * Complete application settings matching `uc-core::settings::model::Settings`.
 *
 * 完整应用设置，匹配 `uc-core::settings::model::Settings`。
 *
 * Field names are snake_case to match the Rust serde serialisation.
 */
export interface Settings {
  schema_version: number
  general: GeneralSettings
  sync: SyncSettings
  retention_policy: RetentionPolicy
  security: SecuritySettings
  pairing: PairingSettings
  keyboard_shortcuts: Record<string, ShortcutKey>
  file_sync: FileSyncSettings
}

// ── API response wrappers ──────────────────────────────────────

/** GET /settings response shape. / GET /settings 响应结构。 */
interface SettingsGetResponse {
  data: Settings
  ts: number
}

/** PUT /settings response shape. / PUT /settings 响应结构。 */
interface SettingsUpdateResponse {
  data: { success: boolean }
  ts: number
}

interface SettingsPatchRequest {
  general?: Partial<GeneralSettings>
  sync?: Partial<SyncSettings>
  retention_policy?: Partial<RetentionPolicy>
  security?: {
    encryption_enabled?: boolean
    auto_unlock_enabled?: boolean
    passphrase?: string
  }
  pairing?: {
    step_timeout?: number
    user_verification_timeout?: number
    session_timeout?: number
    max_retries?: number
  }
  keyboard_shortcuts?: {
    shortcuts: Record<string, ShortcutKey>
  }
  file_sync?: Partial<FileSyncSettings>
}

// ── Public API ─────────────────────────────────────────────────

/**
 * Fetch the current application settings from the daemon.
 *
 * 从 daemon 获取当前应用设置。
 *
 * @returns The full Settings object.
 * @throws {DaemonApiError} On HTTP or session errors.
 */
export async function getSettings(): Promise<Settings> {
  const res = await daemonClient.request<SettingsGetResponse>('/settings')
  return res.data
}

/**
 * Update application settings via deep merge on the server.
 *
 * 通过服务器端深度合并更新应用设置。
 *
 * Only the provided fields are changed; omitted fields retain their current
 * values. Nested objects are merged recursively.
 *
 * @param settings Partial settings payload.
 * @throws {DaemonApiError} On HTTP or validation errors.
 */
export async function updateSettings(settings: Partial<Settings>): Promise<void> {
  const patch = toSettingsPatchRequest(settings)
  await daemonClient.request<SettingsUpdateResponse>('/settings', {
    method: 'PUT',
    body: patch,
  })
}

function toSettingsPatchRequest(settings: Partial<Settings>): SettingsPatchRequest {
  const patch: SettingsPatchRequest = {}

  if (settings.general) {
    const {
      auto_start,
      silent_start,
      auto_check_update,
      theme,
      theme_color,
      language,
      device_name,
      update_channel,
    } = settings.general

    patch.general = {
      auto_start,
      silent_start,
      auto_check_update,
      theme,
      theme_color,
      language,
      device_name,
      update_channel,
    }
  }

  if (settings.sync) {
    const { auto_sync, sync_frequency, content_types, max_file_size_mb } = settings.sync
    patch.sync = {
      auto_sync,
      sync_frequency,
      content_types,
      max_file_size_mb,
    }
  }

  if (settings.retention_policy) {
    const { enabled, rules, skip_pinned, evaluation } = settings.retention_policy
    patch.retention_policy = {
      enabled,
      rules,
      skip_pinned,
      evaluation,
    }
  }

  if (settings.security) {
    const { encryption_enabled, auto_unlock_enabled } = settings.security
    patch.security = {
      encryption_enabled,
      auto_unlock_enabled,
    }
  }

  if (settings.pairing) {
    const { step_timeout, user_verification_timeout, session_timeout, max_retries } =
      settings.pairing
    patch.pairing = {
      step_timeout,
      user_verification_timeout,
      session_timeout,
      max_retries,
    }
  }

  if (settings.keyboard_shortcuts) {
    patch.keyboard_shortcuts = {
      shortcuts: settings.keyboard_shortcuts,
    }
  }

  if (settings.file_sync) {
    const {
      file_sync_enabled,
      small_file_threshold,
      max_file_size,
      file_cache_quota_per_device,
      file_retention_hours,
      file_auto_cleanup,
    } = settings.file_sync
    patch.file_sync = {
      file_sync_enabled,
      small_file_threshold,
      max_file_size,
      file_cache_quota_per_device,
      file_retention_hours,
      file_auto_cleanup,
    }
  }

  return patch
}

/**
 * Storage API module — typed accessors for daemon storage management endpoints.
 *
 * Storage API 模块 — daemon 存储管理端点的类型化访问器。
 *
 * # Endpoints / 端点
 * - `GET /storage/stats` → storage usage statistics
 * - `POST /storage/clear-cache` → clear the blob cache (requires `confirmed: true`)
 */

import { daemonClient } from './client'

// ── Response types ─────────────────────────────────────────────

/**
 * Storage usage statistics returned by `GET /storage/stats`.
 *
 * `GET /storage/stats` 返回的存储使用统计。
 */
export interface StorageStats {
  total_entries: number
  total_size_bytes: number
  cache_size_bytes: number
  oldest_entry_ts: number | null
  newest_entry_ts: number | null
}

// ── Public API ─────────────────────────────────────────────────

/**
 * Fetch storage usage statistics from the daemon.
 *
 * 从 daemon 获取存储使用统计。
 *
 * @returns StorageStats with entry counts, sizes, and timestamp bounds.
 * @throws {DaemonApiError} On HTTP or session errors.
 */
export async function getStorageStats(): Promise<StorageStats> {
  return daemonClient.request<StorageStats>('/storage/stats')
}

/**
 * Request daemon cache clearance.
 *
 * 请求 daemon 清除缓存。
 *
 * The daemon requires `confirmed: true` in the request body. Without it,
 * or with `confirmed: false`, the daemon returns HTTP 400.
 *
 * @param confirmed Must be `true` to trigger the destructive clear operation.
 * @throws {DaemonApiError} On HTTP errors — specifically 400 if `confirmed` is absent/false.
 */
export async function clearCache(confirmed: boolean): Promise<void> {
  await daemonClient.request<void>('/storage/clear-cache', {
    method: 'POST',
    body: { confirmed },
  })
}

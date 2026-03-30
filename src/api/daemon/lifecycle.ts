/**
 * Lifecycle API module — typed accessors for daemon lifecycle endpoints.
 *
 * # Endpoints
 * - `POST /lifecycle/ready` → notify the daemon that the GUI is ready for clipboard capture
 */

import { daemonClient } from './client'

interface LifecycleReadyResponse {
  data?: { success: boolean }
  ts?: number
}

/**
 * Notify the daemon that the GUI is ready and deferred services can start.
 */
export async function signalLifecycleReady(): Promise<void> {
  await daemonClient.request<LifecycleReadyResponse>('/lifecycle/ready', {
    method: 'POST',
  })
}

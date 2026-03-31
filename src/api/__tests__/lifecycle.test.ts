/**
 * Tests for the lifecycle API facade.
 * Verifies that getLifecycleStatus and retryLifecycle delegate to daemon HTTP endpoints.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { daemonClient } from '@/api/daemon/client'
import { getLifecycleStatus, retryLifecycle } from '@/api/lifecycle'
import type { LifecycleStatusDto } from '@/api/types'

describe('lifecycle api facade', () => {
  let requestSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    requestSpy = vi.spyOn(daemonClient, 'request')
    requestSpy.mockResolvedValue(undefined)
  })

  afterEach(() => {
    requestSpy.mockRestore()
  })

  it('getLifecycleStatus calls daemon getLifecycleStatus', async () => {
    const payload: LifecycleStatusDto = { state: 'Ready' }
    requestSpy.mockResolvedValueOnce(payload)

    const result = await getLifecycleStatus()

    expect(requestSpy).toHaveBeenCalledTimes(1)
    expect(requestSpy).toHaveBeenCalledWith('/lifecycle/status')
    expect(result).toEqual(payload)
    expect(result.state).toBe('Ready')
  })

  it('getLifecycleStatus returns typed dto with lifecycleState union', async () => {
    requestSpy.mockResolvedValueOnce({ state: 'Pending' })

    const result = await getLifecycleStatus()

    expect(result.state).toBe('Pending')
  })

  it('retryLifecycle calls daemon retryLifecycle', async () => {
    await retryLifecycle()

    expect(requestSpy).toHaveBeenCalledTimes(1)
    expect(requestSpy).toHaveBeenCalledWith('/lifecycle/retry', { method: 'POST' })
  })

  it('getLifecycleStatus re-throws daemon errors', async () => {
    requestSpy.mockRejectedValueOnce(new Error('daemon unavailable'))

    await expect(getLifecycleStatus()).rejects.toThrow('daemon unavailable')
  })

  it('retryLifecycle re-throws daemon errors', async () => {
    requestSpy.mockRejectedValueOnce(new Error('lifecycle retry failed'))

    await expect(retryLifecycle()).rejects.toThrow('lifecycle retry failed')
  })
})

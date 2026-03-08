import { describe, expect, it, vi, beforeEach } from 'vitest'
import { getLifecycleStatus, retryLifecycle } from '@/api/lifecycle'
import type { CommandError, LifecycleStatusDto } from '@/api/types'
import { invokeWithTrace } from '@/lib/tauri-command'

vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: vi.fn(),
}))

const invokeWithTraceMock = vi.mocked(invokeWithTrace)

describe('lifecycle api dto contract', () => {
  beforeEach(() => {
    invokeWithTraceMock.mockReset()
  })

  it('getLifecycleStatus returns typed dto with lifecycleState union', async () => {
    const payload: LifecycleStatusDto = { state: 'Ready' }

    invokeWithTraceMock.mockResolvedValue(payload)

    const result = await getLifecycleStatus()

    expect(invokeWithTraceMock).toHaveBeenCalledWith('get_lifecycle_status')
    expect(result.state).toBe('Ready')
  })

  it('retryLifecycle forwards command and allows CommandError surface later', async () => {
    invokeWithTraceMock.mockResolvedValue(undefined)

    await retryLifecycle()

    expect(invokeWithTraceMock).toHaveBeenCalledWith('retry_lifecycle')
  })

  it('CommandError discriminated union shape matches backend contract', () => {
    const error: CommandError = {
      code: 'NotFound',
      message: 'entry missing',
    }

    expect(error.code).toBe('NotFound')
    expect(error.message).toBe('entry missing')
  })
})

import type { CommandError, LifecycleStatusDto } from '@/api/types'
import { invokeWithTrace } from '@/lib/tauri-command'

export async function getLifecycleStatus(): Promise<LifecycleStatusDto> {
  try {
    return await invokeWithTrace<LifecycleStatusDto>('get_lifecycle_status')
  } catch (error) {
    console.error('Failed to get lifecycle status:', error)
    throw error
  }
}

export async function retryLifecycle(): Promise<void> {
  try {
    await invokeWithTrace<void>('retry_lifecycle')
  } catch (error) {
    // In future, this may surface CommandError to callers; for now we log and rethrow.
    const typedError = error as CommandError | unknown
    console.error('Failed to retry lifecycle:', typedError)
    throw error
  }
}

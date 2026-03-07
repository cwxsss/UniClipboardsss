import { listen } from '@tauri-apps/api/event'
import { useState, useEffect, useCallback } from 'react'
import { getLifecycleStatus, retryLifecycle } from '@/api/lifecycle'
import type { CommandError, LifecycleStatusDto } from '@/api/types'

export function useLifecycleStatus() {
  const [status, setStatus] = useState<LifecycleStatusDto | null>(null)
  const [retrying, setRetrying] = useState(false)

  useEffect(() => {
    // Check initial status
    getLifecycleStatus()
      .then(setStatus)
      .catch(() => {
        // If the command fails, leave status null (unknown)
        setStatus(null)
      })

    // Listen for lifecycle events
    const unlistenPromise = listen<{ type: string }>('lifecycle://event', () => {
      // Refresh status when lifecycle events occur
      getLifecycleStatus()
        .then(setStatus)
        .catch(() => {})
    })

    return () => {
      unlistenPromise.then(unlisten => unlisten?.())
    }
  }, [])

  const retry = useCallback(async () => {
    setRetrying(true)
    try {
      await retryLifecycle()
      const newStatus = await getLifecycleStatus()
      setStatus(newStatus)
    } catch (error) {
      // Refresh status even on failure; if backend returns CommandError, callers
      // can choose to surface `code`/`message` in the UI later.
      const _typedError = error as CommandError | unknown
      try {
        const newStatus = await getLifecycleStatus()
        setStatus(newStatus)
      } catch {
        // ignore
      }
    } finally {
      setRetrying(false)
    }
  }, [])

  return { status, retry, retrying }
}

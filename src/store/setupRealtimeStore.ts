import { useEffect, useSyncExternalStore } from 'react'
import {
  getSetupState,
  handleSpaceAccessCompleted,
  onSetupStateChanged,
  onSpaceAccessCompleted,
  type SetupState,
} from '@/api/setup'

type SetupRealtimeSnapshot = {
  setupState: SetupState | null
  sessionId: string | null
  hydrated: boolean
}

type SetupRealtimeStore = SetupRealtimeSnapshot & {
  syncSetupStateFromCommand: (nextState: SetupState) => void
}

const RETRY_DELAY_MS = 2000

let snapshot: SetupRealtimeSnapshot = {
  setupState: null,
  sessionId: null,
  hydrated: false,
}

const listeners = new Set<() => void>()
let stopListening: (() => void) | null = null
let stopListeningSpaceAccess: (() => void) | null = null
let startPromise: Promise<void> | null = null
let retryTimer: ReturnType<typeof setTimeout> | null = null
let syncGeneration = 0
let syncPhase: 'idle' | 'starting' | 'running' = 'idle'

function emitChange() {
  listeners.forEach(listener => listener())
}

function isSetupFlowActive(state: SetupState | null): boolean {
  return state !== null && state !== 'Welcome' && state !== 'Completed'
}

function clearRetryTimer() {
  if (!retryTimer) {
    return
  }

  clearTimeout(retryTimer)
  retryTimer = null
}

function updateSnapshot(nextState: SetupState, sessionId?: string | null) {
  snapshot = {
    setupState: nextState,
    sessionId: isSetupFlowActive(nextState) ? (sessionId ?? snapshot.sessionId) : null,
    hydrated: true,
  }
  emitChange()
}

function scheduleRetry() {
  if (retryTimer) {
    return
  }

  retryTimer = setTimeout(() => {
    retryTimer = null
    void ensureSetupRealtimeSync()
  }, RETRY_DELAY_MS)
}

export async function ensureSetupRealtimeSync(): Promise<void> {
  if (syncPhase === 'running') {
    return
  }

  if (startPromise) {
    return startPromise
  }

  syncPhase = 'starting'
  const generation = ++syncGeneration

  startPromise = (async () => {
    try {
      clearRetryTimer()

      if (!snapshot.hydrated) {
        const initialState = await getSetupState()
        if (generation !== syncGeneration) {
          return
        }
        updateSnapshot(initialState, null)
      }

      const unlisten = await onSetupStateChanged(event => {
        if (generation !== syncGeneration) {
          return
        }

        updateSnapshot(event.state, event.sessionId)
      })

      if (generation !== syncGeneration) {
        unlisten()
        return
      }

      const unlistenSpaceAccess = await onSpaceAccessCompleted(async event => {
        if (generation !== syncGeneration) {
          return
        }

        // Skip if setup is already completed (sponsor role — this event fires on both
        // sponsor and joiner sides, but only the joiner needs to finalize setup here).
        if (snapshot.setupState === 'Completed') {
          return
        }

        try {
          const newState = await handleSpaceAccessCompleted()
          updateSnapshot(newState, event.sessionId)
        } catch (error) {
          console.error('Failed to handle space access completed:', error)
        }
      })

      if (generation !== syncGeneration) {
        unlisten()
        unlistenSpaceAccess()
        return
      }

      stopListening = unlisten
      stopListeningSpaceAccess = unlistenSpaceAccess
      syncPhase = 'running'
    } catch (error) {
      if (generation !== syncGeneration) {
        return
      }

      console.error('Failed to initialize setup realtime store:', error)
      syncPhase = 'idle'
      scheduleRetry()
    } finally {
      if (syncPhase !== 'running') {
        startPromise = null
      }
    }
  })()

  return startPromise
}

export function syncSetupStateFromCommand(nextState: SetupState) {
  updateSnapshot(nextState)
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): SetupRealtimeSnapshot {
  return snapshot
}

export function useSetupRealtimeStore(): SetupRealtimeStore {
  const currentSnapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot)

  useEffect(() => {
    void ensureSetupRealtimeSync()
  }, [])

  return {
    ...currentSnapshot,
    syncSetupStateFromCommand,
  }
}

export function resetSetupRealtimeStoreForTests() {
  syncGeneration += 1
  syncPhase = 'idle'
  startPromise = null
  clearRetryTimer()

  if (stopListening) {
    stopListening()
    stopListening = null
  }

  if (stopListeningSpaceAccess) {
    stopListeningSpaceAccess()
    stopListeningSpaceAccess = null
  }

  snapshot = {
    setupState: null,
    sessionId: null,
    hydrated: false,
  }

  emitChange()
}

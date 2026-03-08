import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef, useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { Filter, OrderBy, getClipboardEntry } from '@/api/clipboardItems'
import { getEncryptionSessionStatus } from '@/api/security'
import { toast } from '@/components/ui/toast'
import { useAppDispatch } from '@/store/hooks'
import {
  fetchClipboardItems,
  setNotReady,
  prependItem,
  removeItem,
} from '@/store/slices/clipboardSlice'
import { ClipboardEvent } from '@/types/events'

const THROTTLE_WINDOW_MS = 300
const PAGE_SIZE = 20

interface UseClipboardEventsReturn {
  hasMore: boolean
  handleLoadMore: () => void
  encryptionReady: boolean
}

export function useClipboardEvents(currentFilter: Filter): UseClipboardEventsReturn {
  const dispatch = useAppDispatch()
  const { t } = useTranslation()

  // Refs (no global module state)
  const encryptionReadyRef = useRef<boolean | null>(null)
  const pendingInitialLoadRef = useRef(false)
  const loadInFlightRef = useRef(false)
  const offsetRef = useRef(0)
  const hasMoreRef = useRef(true)
  const currentFilterRef = useRef<Filter>(currentFilter)
  const throttleTimeoutRef = useRef<number | null>(null)
  const lastReloadTimestampRef = useRef<number | undefined>(undefined)

  // State synced with ref
  const [hasMore, setHasMore] = useState(true)
  const [encryptionReady, setEncryptionReady] = useState(false)

  // Load clipboard data
  const loadData = useCallback(
    async ({
      specificFilter,
      reset = false,
    }: { specificFilter?: Filter; reset?: boolean } = {}) => {
      if (loadInFlightRef.current) return
      if (!reset && !hasMoreRef.current) return

      if (reset) {
        offsetRef.current = 0
        hasMoreRef.current = true
        setHasMore(true)
      }

      const filterToUse = specificFilter || currentFilterRef.current
      console.log(t('dashboard.logs.loadingClipboard'), filterToUse)

      loadInFlightRef.current = true
      try {
        const result = await dispatch(
          fetchClipboardItems({
            orderBy: OrderBy.ActiveTimeDesc,
            filter: filterToUse,
            limit: PAGE_SIZE,
            offset: offsetRef.current,
          })
        ).unwrap()

        if (result.status === 'not_ready') {
          hasMoreRef.current = false
          setHasMore(false)
          return
        }

        const fetchedCount = result.items.length
        offsetRef.current += fetchedCount
        const nextHasMore = fetchedCount === PAGE_SIZE
        hasMoreRef.current = nextHasMore
        setHasMore(nextHasMore)
      } catch (error) {
        console.error('Failed to load clipboard data:', error)
        toast.error(t('dashboard.errors.loadFailed'), {
          description: error instanceof Error ? error.message : t('dashboard.errors.unknown'),
        })
      } finally {
        loadInFlightRef.current = false
      }
    },
    [dispatch, t]
  )

  // Filter change effect
  useEffect(() => {
    console.log(t('dashboard.logs.filterChanged'), currentFilter)
    currentFilterRef.current = currentFilter
    if (encryptionReadyRef.current !== true) {
      pendingInitialLoadRef.current = true
      console.log('[useClipboardEvents] Encryption not ready, deferring clipboard load')
      return
    }
    pendingInitialLoadRef.current = false
    loadData({ specificFilter: currentFilter, reset: true })
  }, [currentFilter, loadData, t])

  // Clipboard event listener effect
  useEffect(() => {
    let cancelled = false

    const setupClipboardListener = async () => {
      try {
        const unlisten = await listen<ClipboardEvent>('clipboard://event', event => {
          if (cancelled) return

          if (event.payload.type === 'NewContent' && event.payload.entry_id) {
            // Gate on encryption readiness
            if (encryptionReadyRef.current !== true) {
              console.log('[useClipboardEvents] Encryption not ready, ignoring clipboard event')
              return
            }

            if (event.payload.origin === 'local') {
              // Local event: single-entry query + prepend
              getClipboardEntry(event.payload.entry_id).then(item => {
                if (cancelled) return
                if (item) {
                  dispatch(prependItem(item))
                  offsetRef.current += 1
                } else {
                  // Silent fallback to full reload
                  void loadData({ specificFilter: currentFilterRef.current, reset: true })
                }
              })
            } else {
              // Remote event (or no origin for backward compat): throttled full reload
              const now = Date.now()
              const lastReload = lastReloadTimestampRef.current

              if (lastReload === undefined || now - lastReload >= THROTTLE_WINDOW_MS) {
                lastReloadTimestampRef.current = now

                if (throttleTimeoutRef.current) {
                  clearTimeout(throttleTimeoutRef.current)
                  throttleTimeoutRef.current = null
                }

                void loadData({ specificFilter: currentFilterRef.current, reset: true })
                return
              }

              // Within throttle window: schedule trailing reload
              if (!throttleTimeoutRef.current) {
                const delay = THROTTLE_WINDOW_MS - (now - lastReload)
                throttleTimeoutRef.current = window.setTimeout(() => {
                  lastReloadTimestampRef.current = Date.now()
                  void loadData({ specificFilter: currentFilterRef.current, reset: true })
                  throttleTimeoutRef.current = null
                }, delay)
              }
            }
          } else if (event.payload.type === 'Deleted' && event.payload.entry_id) {
            dispatch(removeItem(event.payload.entry_id))
            offsetRef.current = Math.max(0, offsetRef.current - 1)
          }
        })

        return unlisten
      } catch (err) {
        console.error('[useClipboardEvents] Failed to setup clipboard listener:', err)
        toast.error(t('dashboard.errors.listenerSetupFailed'), {
          description: err instanceof Error ? err.message : t('dashboard.errors.unknown'),
          duration: 5000,
        })
        return undefined
      }
    }

    const unlistenPromise = setupClipboardListener()

    return () => {
      cancelled = true
      if (throttleTimeoutRef.current) {
        clearTimeout(throttleTimeoutRef.current)
        throttleTimeoutRef.current = null
      }
      unlistenPromise.then(unlisten => {
        if (unlisten) unlisten()
      })
    }
  }, [dispatch, loadData, t])

  // Encryption listener effect
  useEffect(() => {
    let cancelled = false

    const setupEncryptionListener = async () => {
      try {
        const unlisten = await listen<'SessionReady' | { type: string }>(
          'encryption://event',
          event => {
            if (cancelled) return

            const eventType = typeof event.payload === 'string' ? event.payload : event.payload.type

            if (eventType === 'SessionReady') {
              console.log('[useClipboardEvents] Encryption session ready, reloading clipboard data')
              encryptionReadyRef.current = true
              setEncryptionReady(true)
              dispatch(setNotReady(false))
              if (pendingInitialLoadRef.current) {
                pendingInitialLoadRef.current = false
              }
              loadData({ specificFilter: currentFilterRef.current, reset: true })
            }
          }
        )

        return unlisten
      } catch (err) {
        console.error('[useClipboardEvents] Failed to setup encryption listener:', err)
        return undefined
      }
    }

    const unlistenPromise = setupEncryptionListener()

    // Check initial encryption status
    const checkEncryptionStatus = async () => {
      try {
        const status = await getEncryptionSessionStatus()
        if (cancelled) return
        if (!status.initialized || status.session_ready) {
          encryptionReadyRef.current = true
          setEncryptionReady(true)
          dispatch(setNotReady(false))
          if (pendingInitialLoadRef.current) {
            pendingInitialLoadRef.current = false
            loadData({ specificFilter: currentFilterRef.current, reset: true })
          }
        } else {
          encryptionReadyRef.current = false
          setEncryptionReady(false)
          dispatch(setNotReady(true))
          console.log(
            '[useClipboardEvents] Encryption initialized but session not ready; waiting for unlock'
          )
        }
      } catch (err) {
        if (cancelled) return
        console.error('[useClipboardEvents] Failed to check encryption session status:', err)
        encryptionReadyRef.current = true
        setEncryptionReady(true)
        dispatch(setNotReady(false))
        if (pendingInitialLoadRef.current) {
          pendingInitialLoadRef.current = false
          loadData({ specificFilter: currentFilterRef.current, reset: true })
        }
      }
    }

    checkEncryptionStatus()

    return () => {
      cancelled = true
      unlistenPromise.then(unlisten => {
        if (unlisten) unlisten()
      })
    }
  }, [dispatch, loadData])

  const handleLoadMore = useCallback(() => {
    if (encryptionReadyRef.current !== true) return
    void loadData()
  }, [loadData])

  return { hasMore, handleLoadMore, encryptionReady }
}

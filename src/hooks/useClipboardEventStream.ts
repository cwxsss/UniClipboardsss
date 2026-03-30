import { useEffect, useRef } from 'react'
import { daemonWs } from '@/lib/daemon-ws'
import { getClipboardEntry } from '@/api/clipboardItems'
import type { ClipboardItemResponse } from '@/api/clipboardItems'

export interface UseClipboardEventStreamOptions {
  enabled?: boolean
  throttleMs?: number
  onLocalItem: (item: ClipboardItemResponse) => void
  onRemoteInvalidate: () => void
  onDeleted: (id: string) => void
}

/**
 * Payload for `clipboard.new-content` daemon WS events.
 * (Matches the Rust `ClipboardNewContentEvent` serde shape.)
 */
interface ClipboardNewContentPayload {
  entry_id: string
  preview: string
  origin: string // "local" | "remote"
}

interface ClipboardDeletedPayload {
  entry_id: string
}

export function useClipboardEventStream({
  enabled = true,
  throttleMs = 300,
  onLocalItem,
  onRemoteInvalidate,
  onDeleted,
}: UseClipboardEventStreamOptions): void {
  const timeoutRef = useRef<number | null>(null)
  const lastReloadTimestampRef = useRef<number | undefined>(undefined)
  const onLocalItemRef = useRef(onLocalItem)
  const onRemoteInvalidateRef = useRef(onRemoteInvalidate)
  const onDeletedRef = useRef(onDeleted)

  useEffect(() => {
    onLocalItemRef.current = onLocalItem
    onRemoteInvalidateRef.current = onRemoteInvalidate
    onDeletedRef.current = onDeleted
  }, [onDeleted, onLocalItem, onRemoteInvalidate])

  useEffect(() => {
    if (!enabled) return

    const handler = (event: { topic: string; eventType: string; payload: unknown }) => {
      // Route clipboard.new-content to onLocalItem / onRemoteInvalidate.
      if (event.eventType === 'clipboard.new-content') {
        const payload = event.payload as ClipboardNewContentPayload
        if (payload.origin === 'local') {
          void getClipboardEntry(payload.entry_id).then(item => {
            if (!item) return
            onLocalItemRef.current(item)
          })
          return
        }

        // Remote: throttled full list reload.
        const now = Date.now()
        const lastReload = lastReloadTimestampRef.current
        if (lastReload === undefined || now - lastReload >= throttleMs) {
          lastReloadTimestampRef.current = now
          if (timeoutRef.current) {
            clearTimeout(timeoutRef.current)
            timeoutRef.current = null
          }
          onRemoteInvalidateRef.current()
          return
        }

        if (!timeoutRef.current) {
          const delay = throttleMs - (now - lastReload)
          timeoutRef.current = window.setTimeout(() => {
            lastReloadTimestampRef.current = Date.now()
            onRemoteInvalidateRef.current()
            timeoutRef.current = null
          }, delay)
        }
        return
      }

      // Route clipboard.deleted to onDeleted.
      if (event.eventType === 'clipboard.deleted') {
        const payload = event.payload as ClipboardDeletedPayload
        if (payload.entry_id) {
          onDeletedRef.current(payload.entry_id)
        }
        return
      }
    }

    const unsubscribe = daemonWs.subscribe(['clipboard'], handler)

    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current)
        timeoutRef.current = null
      }
      unsubscribe()
    }
  }, [enabled, throttleMs])
}

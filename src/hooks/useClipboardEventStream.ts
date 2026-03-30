import { useEffect, useRef } from 'react'
import { daemonWs } from '@/lib/daemon-ws'
import { getClipboardEntries, isImageType } from '@/api/clipboardItems'
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

// ── Daemon DTO → Frontend response transformer ──────────────────
// Mirrors the transformProjectionToResponse logic from clipboardItems.ts
// so useClipboardEventStream uses the same transformation as clipboardSlice.
function extractDomainFromUrl(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return url
  }
}

function transformDtoToItemResponse(entry: import('@/api/daemon/clipboard').ClipboardEntryDto): ClipboardItemResponse {
  const isFile = entry.content_type.includes('uri-list')
  const isImage = !isFile && isImageType(entry.content_type)
  const hasLinkData = !isImage && entry.link_urls && entry.link_urls.length > 0

  let linkItem: { urls: string[]; domains: string[] } | null = null
  if (hasLinkData) {
    linkItem = {
      urls: entry.link_urls!,
      domains: entry.link_domains ?? entry.link_urls!.map(extractDomainFromUrl),
    }
  }

  return {
    id: entry.id,
    is_downloaded: true,
    is_favorited: entry.is_favorited,
    created_at: entry.captured_at,
    updated_at: entry.updated_at,
    active_time: entry.active_time,
    item: {
      text:
        !isImage && !isFile && !hasLinkData
          ? { display_text: entry.preview, has_detail: entry.has_detail, size: entry.size_bytes }
          : null,
      image: isImage
        ? { thumbnail: entry.thumbnail_url ?? null, size: entry.size_bytes, width: 0, height: 0 }
        : null,
      file: isFile
        ? {
            file_names: entry.preview
              .split('\n')
              .filter(Boolean)
              .map(uri => {
                try {
                  return decodeURIComponent(new URL(uri).pathname.split('/').pop() || uri)
                } catch {
                  return uri
                }
              }),
            file_sizes: entry.file_sizes ?? [],
          }
        : null,
      link: linkItem as unknown as ClipboardItemResponse['item']['link'],
      code: null,
      unknown: null,
    },
    file_transfer_status: entry.file_transfer_status ?? null,
    file_transfer_reason: entry.file_transfer_reason ?? null,
  }
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
          // Fetch single entry from daemon list endpoint (matching clipboardSlice pattern)
          void getClipboardEntries(50, 0)
            .then(response => {
              if (response.status !== 'ready' || !response.entries) return null
              const entry = response.entries.find(e => e.id === payload.entry_id)
              if (!entry) return null
              return transformDtoToItemResponse(entry)
            })
            .then(item => {
              if (item) onLocalItemRef.current(item)
            })
            .catch(err => console.error('Failed to fetch local clipboard entry:', err))
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

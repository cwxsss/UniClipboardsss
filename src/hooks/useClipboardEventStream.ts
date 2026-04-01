import { useEffect, useRef } from 'react'
import { isImageType } from '@/api/clipboardItems'
import type { ClipboardItemResponse } from '@/api/clipboardItems'
import { getClipboardEntries } from '@/api/daemon/clipboard'
import { daemonWs } from '@/lib/daemon-ws'

export interface UseClipboardEventStreamOptions {
  enabled?: boolean
  throttleMs?: number
  onLocalItem: (item: ClipboardItemResponse) => void
  onRemoteInvalidate: () => void
  onDeleted: (id: string) => void
}

/**
 * Payload for `clipboard.new_content` daemon WS events.
 * (Matches the Rust `ClipboardNewContentEvent` serde shape.)
 */
interface ClipboardNewContentPayload {
  entryId: string
  preview: string
  origin: string // "local" | "remote"
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

function transformDtoToItemResponse(
  entry: import('@/api/daemon/clipboard').ClipboardEntryDto
): ClipboardItemResponse {
  const isFile = entry.contentType.includes('uri-list')
  const isImage = !isFile && isImageType(entry.contentType)
  const hasLinkData = !isImage && entry.linkUrls && entry.linkUrls.length > 0

  let linkItem: { urls: string[]; domains: string[] } | null = null
  if (hasLinkData) {
    linkItem = {
      urls: entry.linkUrls!,
      domains: entry.linkDomains ?? entry.linkUrls!.map(extractDomainFromUrl),
    }
  }

  return {
    id: entry.id,
    is_downloaded: true,
    is_favorited: entry.isFavorited,
    created_at: entry.capturedAt,
    updated_at: entry.updatedAt,
    active_time: entry.activeTime,
    item: {
      text:
        !isImage && !isFile && !hasLinkData
          ? { display_text: entry.preview, has_detail: entry.hasDetail, size: entry.sizeBytes }
          : null,
      image: isImage
        ? { thumbnail: entry.thumbnailUrl ?? null, size: entry.sizeBytes, width: 0, height: 0 }
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
            file_sizes: entry.fileSizes ?? [],
          }
        : null,
      link: linkItem as unknown as ClipboardItemResponse['item']['link'],
      code: null,
      unknown: null,
    },
    file_transfer_status: entry.fileTransferStatus ?? null,
    file_transfer_reason: entry.fileTransferReason ?? null,
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
    if (!enabled) {
      console.warn(
        '[useClipboardEventStream] disabled (enabled=false, likely encryptionReady=false)'
      )
      return
    }
    console.info('[useClipboardEventStream] subscribing to clipboard topic')

    const handler = (event: { topic: string; eventType: string; payload: unknown }) => {
      console.info('[useClipboardEventStream] received event:', event.eventType, event.payload)
      // Route clipboard.new_content to onLocalItem / onRemoteInvalidate.
      if (event.eventType === 'clipboard.new_content') {
        const payload = event.payload as ClipboardNewContentPayload
        console.info(
          '[useClipboardEventStream] clipboard.new_content payload:',
          JSON.stringify(payload)
        )
        if (payload.origin === 'local') {
          // Fetch single entry from daemon list endpoint (matching clipboardSlice pattern)
          void getClipboardEntries(50, 0)
            .then(response => {
              console.info(
                '[useClipboardEventStream] getClipboardEntries response status:',
                response.status,
                'entries count:',
                response.entries?.length ?? 0
              )
              if (response.status !== 'ready' || !response.entries) return null
              const entry = response.entries.find(e => e.id === payload.entryId)
              console.info(
                '[useClipboardEventStream] found entry for id',
                payload.entryId,
                ':',
                !!entry
              )
              if (!entry) return null
              return transformDtoToItemResponse(entry)
            })
            .then(item => {
              console.info('[useClipboardEventStream] onLocalItem called with item:', !!item)
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

      // Note: clipboard.deleted is never emitted by the daemon.
      // The onDeleted callback is retained for API symmetry but will never fire.
      void onDeletedRef
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

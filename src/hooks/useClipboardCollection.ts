import { useCallback, useEffect, useState } from 'react'
import { useClipboardEventStream } from './useClipboardEventStream'
import { useEncryptionSessionState } from './useEncryptionSessionState'
import { getClipboardEntries } from '@/api/daemon/clipboard'
import { isImageType } from '@/api/clipboardItems'
import type { ClipboardItemResponse } from '@/api/clipboardItems'

const PAGE_SIZE = 50

// ── Daemon DTO → Frontend response transformer ──────────────────
// Mirrors the transformProjectionToResponse logic from clipboardItems.ts
// so useClipboardCollection uses the same transformation as clipboardSlice.
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

export interface ClipboardCollectionResult {
  items: ClipboardItemResponse[]
  loading: boolean
  isLocked: boolean
  encryptionReady: boolean
  reload: () => Promise<void>
}

export function useClipboardCollection(): ClipboardCollectionResult {
  const { encryptionReady, isLocked } = useEncryptionSessionState()
  const [items, setItems] = useState<ClipboardItemResponse[]>([])
  const [loading, setLoading] = useState(true)

  const reload = useCallback(async () => {
    if (!encryptionReady) {
      setItems([])
      setLoading(false)
      return
    }

    setLoading(true)
    try {
      const result = await getClipboardEntries(PAGE_SIZE, 0)
      if (result.status === 'not_ready') {
        setItems([])
        return
      }
      const transformedItems = result.entries?.map(transformDtoToItemResponse) ?? []
      setItems(transformedItems)
    } catch (err) {
      console.error('Failed to load clipboard items:', err)
    } finally {
      setLoading(false)
    }
  }, [encryptionReady])

  useEffect(() => {
    if (!encryptionReady) {
      setItems([])
      setLoading(false)
      return
    }

    void reload()
  }, [encryptionReady, reload])

  useClipboardEventStream({
    enabled: encryptionReady,
    onLocalItem: item => {
      setItems(prev => (prev.some(existing => existing.id === item.id) ? prev : [item, ...prev]))
    },
    onRemoteInvalidate: () => {
      void reload()
    },
    onDeleted: id => {
      setItems(prev => prev.filter(item => item.id !== id))
    },
  })

  return { items, loading, isLocked, encryptionReady, reload }
}

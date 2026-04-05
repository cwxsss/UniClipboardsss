import { useCallback, useEffect, useState } from 'react'
import { useClipboardEventStream } from './useClipboardEventStream'
import { useEncryptionSessionState } from './useEncryptionSessionState'
import { isImageType } from '@/api/clipboardItems'
import type { ClipboardItemResponse } from '@/api/clipboardItems'
import { getClipboardEntries } from '@/api/daemon/clipboard'

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
      setLoading(!isLocked)
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
  }, [encryptionReady, isLocked])

  useEffect(() => {
    if (!encryptionReady) {
      setItems([])
      setLoading(!isLocked)
      return
    }

    void reload()
  }, [encryptionReady, isLocked, reload])

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

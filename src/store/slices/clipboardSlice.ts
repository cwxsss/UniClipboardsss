import { createSlice, createAsyncThunk, PayloadAction } from '@reduxjs/toolkit'
import { hydrateEntryTransferStatuses } from './fileTransferSlice'
import type { ClipboardItemResponse, ClipboardItemsResult } from '@/api/clipboardItems'
import {
  // Tauri API — kept as commented reference during transition
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  // getClipboardItems as getClipboardItemsTauri,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  // deleteClipboardItem as deleteClipboardItemTauri,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  // copyClipboardItem as copyClipboardItemTauri,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  // clearClipboardItems as clearClipboardItemsTauri,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  // favoriteClipboardItem as favoriteClipboardItemTauri,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  // unfavoriteClipboardItem as unfavoriteClipboardItemTauri,
  OrderBy,
  Filter,
} from '@/api/clipboardItems'
import {
  getClipboardEntries,
  deleteClipboardEntry,
  restoreClipboardEntry,
  toggleFavorite,
  // clearClipboardItems — no daemon endpoint yet, see comment in thunk
} from '@/api/daemon'

// ── Daemon DTO → Frontend response transformer ──────────────────
// Mirrors the transformProjectionToResponse logic from clipboardItems.ts
// so clipboardSlice remains independent of the old Tauri module.
function isImageType(contentType: string): boolean {
  return contentType === 'image' || contentType.startsWith('image/')
}

function extractDomainFromUrl(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return url
  }
}

function transformDtoToItemResponse(entry: import('@/api/daemon').ClipboardEntryDto): ClipboardItemResponse {
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

// ── State ────────────────────────────────────────────────────────
interface ClipboardState {
  items: ClipboardItemResponse[]
  loading: boolean
  notReady: boolean
  error: string | null
  deleteConfirmId: string | null
  staleEntryIds: string[]
}

// 初始状态
const initialState: ClipboardState = {
  items: [],
  loading: false,
  notReady: false,
  error: null,
  deleteConfirmId: null,
  staleEntryIds: [],
}

// 定义获取剪贴板项目的参数接口
interface FetchClipboardItemsParams {
  orderBy?: OrderBy
  limit?: number
  offset?: number
  isFavorited?: boolean
  filter?: Filter
}

type ClipboardItemsResultWithOffset = ClipboardItemsResult & { offset: number }
type FetchClipboardItemsAction = {
  payload: ClipboardItemsResultWithOffset
  type: string
  meta: { arg?: FetchClipboardItemsParams }
}

// 异步 Thunk Actions
export const fetchClipboardItems = createAsyncThunk<
  ClipboardItemsResultWithOffset,
  FetchClipboardItemsParams | undefined
>('clipboard/fetchItems', async (params = {}, { rejectWithValue, dispatch }) => {
  try {
    // Daemon API: GET /clipboard/entries
    // Note: orderBy and filter are not yet supported by the daemon endpoint.
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { orderBy: _orderBy, filter: _filter, isFavorited: _isFavorited, ...rest } = params
    const result = await getClipboardEntries(rest.limit ?? 50, rest.offset ?? 0)

    // Hydrate durable file transfer statuses from persisted API fields.
    // This ensures entryStatusById in fileTransferSlice is seeded on app load
    // so file entries show correct status badges immediately after restart.
    if (result.status === 'ready' && result.entries) {
      const statusEntries = result.entries
        .filter(item => item.file_transfer_status != null)
        .map(item => ({
          entryId: item.id,
          status: item.file_transfer_status as 'pending' | 'transferring' | 'completed' | 'failed',
          reason: item.file_transfer_reason ?? null,
        }))
      if (statusEntries.length > 0) {
        dispatch(hydrateEntryTransferStatuses(statusEntries))
      }
    }

    // Transform daemon ClipboardEntriesResponse to ClipboardItemsResult shape
    if (result.status === 'not_ready') {
      return { status: 'not_ready', items: [], offset: params.offset ?? 0 }
    }
    const items: ClipboardItemResponse[] = result.entries?.map(transformDtoToItemResponse) ?? []
    return { ...result, items, status: 'ready' as const, offset: params.offset ?? 0 }
  } catch {
    return rejectWithValue('获取剪贴板内容失败')
  }
})

export const removeClipboardItem = createAsyncThunk(
  'clipboard/removeItem',
  async (id: string, { rejectWithValue }) => {
    try {
      // Daemon API: DELETE /clipboard/entries/:id
      await deleteClipboardEntry(id)
      return id
    } catch {
      return rejectWithValue('删除剪贴板内容失败')
    }
  }
)

export const toggleFavoriteItem = createAsyncThunk(
  'clipboard/toggleFavorite',
  async ({ id, isFavorited }: { id: string; isFavorited: boolean }, { rejectWithValue }) => {
    try {
      // Daemon API: PUT /clipboard/entries/:id/favorite { is_favorited }
      await toggleFavorite(id, isFavorited)
      return { id, isFavorited }
    } catch {
      return rejectWithValue('设置收藏状态失败')
    }
  }
)

export const clearAllItems = createAsyncThunk(
  'clipboard/clearAll',
  async (_, { rejectWithValue }) => {
    try {
      // Daemon API: POST /clipboard/entries/clear
      // Note: We call the thunk without awaiting its result - the result contains
      // { deletedCount, failedEntries } but we just need success/failure
      const { clearClipboardHistory } = await import('@/api/daemon/clipboard')
      await clearClipboardHistory()
      return true
    } catch {
      return rejectWithValue('清空剪贴板内容失败')
    }
  }
)

export const copyToClipboard = createAsyncThunk(
  'clipboard/copyItem',
  async (id: string, { rejectWithValue }) => {
    try {
      // Daemon API: POST /clipboard/restore/:id
      await restoreClipboardEntry(id)
      return { id, success: true }
    } catch {
      return rejectWithValue('复制到剪贴板失败')
    }
  }
)

// 创建 Slice
const clipboardSlice = createSlice({
  name: 'clipboard',
  initialState,
  reducers: {
    setDeleteConfirmId: (state, action: PayloadAction<string | null>) => {
      state.deleteConfirmId = action.payload
    },
    setNotReady: (state, action: PayloadAction<boolean>) => {
      state.notReady = action.payload
      if (action.payload) {
        state.loading = false
        state.error = null
      }
    },
    clearError: state => {
      state.error = null
    },
    prependItem: (state, action: PayloadAction<ClipboardItemResponse>) => {
      if (state.items.some(item => item.id === action.payload.id)) return
      state.items.unshift(action.payload)
    },
    removeItem: (state, action: PayloadAction<string>) => {
      state.items = state.items.filter(item => item.id !== action.payload)
    },
    resetItems: state => {
      state.items = []
      state.error = null
    },
    markEntryStale: (state, action: PayloadAction<string>) => {
      if (!state.staleEntryIds.includes(action.payload)) {
        state.staleEntryIds.push(action.payload)
      }
    },
    clearStaleEntries: state => {
      state.staleEntryIds = []
    },
  },
  extraReducers: builder => {
    // 处理获取剪贴板内容
    builder.addCase(fetchClipboardItems.pending, state => {
      // Only show loading state when there are no cached items.
      // When items already exist (e.g., navigating back to the page),
      // we fetch in the background without triggering skeleton/loading UI.
      if (state.items.length === 0) {
        state.loading = true
      }
      state.error = null
      state.notReady = false
    })
    builder.addCase(fetchClipboardItems.fulfilled, (state, action: FetchClipboardItemsAction) => {
      state.loading = false
      if (action.payload.status === 'not_ready') {
        state.notReady = true
        return
      }

      state.notReady = false
      const pageOffset = action.payload.offset
      if (pageOffset > 0 && state.items.length > 0) {
        const existingIds = new Set(state.items.map(item => item.id))
        for (const item of action.payload.items) {
          if (!existingIds.has(item.id)) {
            state.items.push(item)
          }
        }
      } else {
        state.items = action.payload.items
      }
    })
    builder.addCase(fetchClipboardItems.rejected, (state, action) => {
      state.loading = false
      state.error = action.payload as string
      state.notReady = false
    })

    // 处理删除剪贴板内容
    builder.addCase(removeClipboardItem.fulfilled, (state, action) => {
      state.items = state.items.filter(item => item.id !== action.payload)
      state.deleteConfirmId = null
    })
    builder.addCase(removeClipboardItem.rejected, (state, action) => {
      state.error = action.payload as string
    })

    // 处理收藏状态切换
    builder.addCase(toggleFavoriteItem.fulfilled, (state, action) => {
      const { id, isFavorited } = action.payload
      const item = state.items.find(item => item.id === id)
      if (item) {
        item.is_favorited = isFavorited
      }
    })
    builder.addCase(toggleFavoriteItem.rejected, (state, action) => {
      state.error = action.payload as string
    })

    // 处理清空剪贴板
    builder.addCase(clearAllItems.fulfilled, state => {
      state.items = []
    })
    builder.addCase(clearAllItems.rejected, (state, action) => {
      state.error = action.payload as string
    })

    // 处理复制到剪贴板
    builder.addCase(copyToClipboard.rejected, (state, action) => {
      state.error = action.payload as string
    })
  },
})

// 导出 Actions
export const {
  setDeleteConfirmId,
  setNotReady,
  clearError,
  prependItem,
  removeItem,
  resetItems,
  markEntryStale,
  clearStaleEntries,
} = clipboardSlice.actions

// 导出 Reducer
export default clipboardSlice.reducer

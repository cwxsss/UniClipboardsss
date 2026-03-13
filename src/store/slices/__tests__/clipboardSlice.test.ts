import { describe, it, expect } from 'vitest'
import clipboardReducer, { prependItem, removeItem } from '../clipboardSlice'
import type { ClipboardItemResponse } from '@/api/clipboardItems'

function makeItem(id: string): ClipboardItemResponse {
  return {
    id,
    is_downloaded: true,
    is_favorited: false,
    created_at: Date.now(),
    updated_at: Date.now(),
    active_time: 0,
    item: {
      text: { display_text: `text-${id}`, has_detail: false, size: 10 },
      image: null,
      file: null,
      link: null,
      code: null,
      unknown: null,
    },
  }
}

const initialState = {
  items: [],
  loading: false,
  notReady: false,
  error: null,
  deleteConfirmId: null,
  staleEntryIds: [],
}

describe('clipboardSlice reducers', () => {
  describe('prependItem', () => {
    it('inserts item at index 0', () => {
      const existing = makeItem('a')
      const state = { ...initialState, items: [existing] }
      const newItem = makeItem('b')

      const result = clipboardReducer(state, prependItem(newItem))

      expect(result.items).toHaveLength(2)
      expect(result.items[0].id).toBe('b')
      expect(result.items[1].id).toBe('a')
    })

    it('does not insert duplicate entry_id', () => {
      const existing = makeItem('a')
      const state = { ...initialState, items: [existing] }
      const duplicate = makeItem('a')

      const result = clipboardReducer(state, prependItem(duplicate))

      expect(result.items).toHaveLength(1)
    })

    it('inserts into empty items array', () => {
      const newItem = makeItem('first')

      const result = clipboardReducer(initialState, prependItem(newItem))

      expect(result.items).toHaveLength(1)
      expect(result.items[0].id).toBe('first')
    })
  })

  describe('removeItem', () => {
    it('removes item by entry_id', () => {
      const items = [makeItem('a'), makeItem('b'), makeItem('c')]
      const state = { ...initialState, items }

      const result = clipboardReducer(state, removeItem('b'))

      expect(result.items).toHaveLength(2)
      expect(result.items.map(i => i.id)).toEqual(['a', 'c'])
    })

    it('leaves state unchanged for non-existent entry_id', () => {
      const items = [makeItem('a'), makeItem('b')]
      const state = { ...initialState, items }

      const result = clipboardReducer(state, removeItem('z'))

      expect(result.items).toHaveLength(2)
    })
  })

  describe('initial state', () => {
    it('has correct shape', () => {
      const state = clipboardReducer(undefined, { type: 'unknown' })

      expect(state.items).toEqual([])
      expect(state.loading).toBe(false)
      expect(state.notReady).toBe(false)
      expect(state.error).toBeNull()
      expect(state.deleteConfirmId).toBeNull()
    })
  })
})

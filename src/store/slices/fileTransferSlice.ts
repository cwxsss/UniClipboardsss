import { createSlice, PayloadAction } from '@reduxjs/toolkit'
import type { RootState } from '../index'

export interface TransferProgressInfo {
  transferId: string
  entryId: string | null
  peerId: string
  direction: 'Sending' | 'Receiving'
  chunksCompleted: number
  totalChunks: number
  bytesTransferred: number
  totalBytes: number | null
  status: 'active' | 'completed' | 'failed'
  startedAt: number
  updatedAt: number
}

interface FileTransferState {
  activeTransfers: Record<string, TransferProgressInfo>
  entryTransferMap: Record<string, string>
}

const initialState: FileTransferState = {
  activeTransfers: {},
  entryTransferMap: {},
}

interface UpdateTransferProgressPayload {
  transferId: string
  peerId: string
  direction: 'Sending' | 'Receiving'
  chunksCompleted: number
  totalChunks: number
  bytesTransferred: number
  totalBytes?: number | null
}

const fileTransferSlice = createSlice({
  name: 'fileTransfer',
  initialState,
  reducers: {
    updateTransferProgress(state, action: PayloadAction<UpdateTransferProgressPayload>) {
      const {
        transferId,
        peerId,
        direction,
        chunksCompleted,
        totalChunks,
        bytesTransferred,
        totalBytes,
      } = action.payload
      const now = Date.now()
      const existing = state.activeTransfers[transferId]

      const isCompleted = chunksCompleted === totalChunks && totalChunks > 0

      state.activeTransfers[transferId] = {
        transferId,
        entryId: existing?.entryId ?? null,
        peerId,
        direction,
        chunksCompleted,
        totalChunks,
        bytesTransferred,
        totalBytes: totalBytes ?? null,
        status: isCompleted ? 'completed' : 'active',
        startedAt: existing?.startedAt ?? now,
        updatedAt: now,
      }
    },

    linkTransferToEntry(state, action: PayloadAction<{ transferId: string; entryId: string }>) {
      const { transferId, entryId } = action.payload
      const transfer = state.activeTransfers[transferId]
      if (transfer) {
        transfer.entryId = entryId
      }
      state.entryTransferMap[entryId] = transferId
    },

    markTransferFailed(state, action: PayloadAction<{ transferId: string }>) {
      const transfer = state.activeTransfers[action.payload.transferId]
      if (transfer) {
        transfer.status = 'failed'
        transfer.updatedAt = Date.now()
      }
    },

    clearCompletedTransfers(state) {
      const toRemove: string[] = []
      for (const [id, transfer] of Object.entries(state.activeTransfers)) {
        if (transfer.status === 'completed') {
          toRemove.push(id)
        }
      }
      for (const id of toRemove) {
        const transfer = state.activeTransfers[id]
        if (transfer?.entryId) {
          delete state.entryTransferMap[transfer.entryId]
        }
        delete state.activeTransfers[id]
      }
    },

    removeTransfer(state, action: PayloadAction<string>) {
      const transferId = action.payload
      const transfer = state.activeTransfers[transferId]
      if (transfer?.entryId) {
        delete state.entryTransferMap[transfer.entryId]
      }
      delete state.activeTransfers[transferId]
    },
  },
})

export const {
  updateTransferProgress,
  linkTransferToEntry,
  markTransferFailed,
  clearCompletedTransfers,
  removeTransfer,
} = fileTransferSlice.actions

// Selectors
export const selectTransferByEntryId = (
  state: RootState,
  entryId: string
): TransferProgressInfo | undefined => {
  const transferId = state.fileTransfer.entryTransferMap[entryId]
  if (!transferId) return undefined
  return state.fileTransfer.activeTransfers[transferId]
}

export const selectActiveTransfers = (state: RootState): TransferProgressInfo[] => {
  return Object.values(state.fileTransfer.activeTransfers).filter(t => t.status === 'active')
}

export const selectIsEntryTransferring = (state: RootState, entryId: string): boolean => {
  const transfer = selectTransferByEntryId(state, entryId)
  return transfer?.status === 'active'
}

export default fileTransferSlice.reducer

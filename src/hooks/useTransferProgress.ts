import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef } from 'react'
import { useAppDispatch } from '@/store/hooks'
import {
  updateTransferProgress,
  removeTransfer,
  markTransferFailed,
  cancelClipboardWrite,
  setEntryTransferStatus,
} from '@/store/slices/fileTransferSlice'

const COMPLETED_CLEAR_DELAY_MS = 3000

interface TransferProgressEvent {
  transferId: string
  peerId: string
  direction: 'Sending' | 'Receiving'
  chunksCompleted: number
  totalChunks: number
  bytesTransferred: number
  totalBytes?: number | null
}

interface FileTransferStatusEvent {
  transferId: string
  entryId: string
  status: string
  reason?: string | null
}

/**
 * Hook that listens to file-transfer:// Tauri events and dispatches
 * progress updates and durable status changes to the Redux fileTransfer slice.
 *
 * Call once in a top-level component (e.g., ClipboardContent) to activate.
 */
export function useTransferProgress(): void {
  const dispatch = useAppDispatch()
  const clearTimeoutsRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map())

  useEffect(() => {
    let cancelled = false

    const setup = async () => {
      try {
        // Listen for durable status-changed events (pending/transferring/completed/failed)
        const unlistenStatusChanged = await listen<FileTransferStatusEvent>(
          'file-transfer://status-changed',
          event => {
            if (cancelled) return
            const { entryId, status, reason } = event.payload
            const validStatuses = ['pending', 'transferring', 'completed', 'failed'] as const
            if (validStatuses.includes(status as (typeof validStatuses)[number])) {
              dispatch(
                setEntryTransferStatus({
                  entryId,
                  status: status as (typeof validStatuses)[number],
                  reason: reason ?? null,
                })
              )
            }

            // If status is failed, also mark the transfer as failed in progress state
            if (status === 'failed') {
              dispatch(
                markTransferFailed({
                  transferId: event.payload.transferId,
                  error: reason ?? undefined,
                })
              )
            }
          }
        )

        // Listen for clipboard changes to cancel auto-write on active transfers
        const unlistenClipboard = await listen('clipboard://new-content', () => {
          if (cancelled) return
          dispatch(cancelClipboardWrite())
        })

        const unlisten = await listen<TransferProgressEvent>('file-transfer://progress', event => {
          if (cancelled) return

          const payload = event.payload
          dispatch(
            updateTransferProgress({
              transferId: payload.transferId,
              peerId: payload.peerId,
              direction: payload.direction,
              chunksCompleted: payload.chunksCompleted,
              totalChunks: payload.totalChunks,
              bytesTransferred: payload.bytesTransferred,
              totalBytes: payload.totalBytes,
            })
          )

          // Auto-clear completed transfers after delay
          // NOTE: this only clears the ephemeral progress state, NOT the durable entryStatusById
          const isCompleted =
            payload.chunksCompleted === payload.totalChunks && payload.totalChunks > 0
          if (isCompleted) {
            // Clear any existing timeout for this transfer
            const existing = clearTimeoutsRef.current.get(payload.transferId)
            if (existing) clearTimeout(existing)

            const timeout = setTimeout(() => {
              if (!cancelled) {
                dispatch(removeTransfer(payload.transferId))
              }
              clearTimeoutsRef.current.delete(payload.transferId)
            }, COMPLETED_CLEAR_DELAY_MS)

            clearTimeoutsRef.current.set(payload.transferId, timeout)
          }
        })

        return { unlisten, unlistenStatusChanged, unlistenClipboard }
      } catch (err) {
        console.error('[useTransferProgress] Failed to setup transfer progress listener:', err)
        return undefined
      }
    }

    const setupPromise = setup()

    return () => {
      cancelled = true
      // Clear all pending timeouts
      for (const timeout of clearTimeoutsRef.current.values()) {
        clearTimeout(timeout)
      }
      clearTimeoutsRef.current.clear()
      // Unlisten all
      setupPromise.then(listeners => {
        if (listeners) {
          listeners.unlisten()
          listeners.unlistenStatusChanged()
          listeners.unlistenClipboard()
        }
      })
    }
  }, [dispatch])
}

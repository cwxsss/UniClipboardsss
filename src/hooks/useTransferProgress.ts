import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef } from 'react'
import { useAppDispatch } from '@/store/hooks'
import { updateTransferProgress, removeTransfer } from '@/store/slices/fileTransferSlice'

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

/**
 * Hook that listens to transfer://progress Tauri events and dispatches
 * progress updates to the Redux fileTransfer slice.
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
        const unlisten = await listen<TransferProgressEvent>('transfer://progress', event => {
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

        return unlisten
      } catch (err) {
        console.error('[useTransferProgress] Failed to setup transfer progress listener:', err)
        return undefined
      }
    }

    const unlistenPromise = setup()

    return () => {
      cancelled = true
      // Clear all pending timeouts
      for (const timeout of clearTimeoutsRef.current.values()) {
        clearTimeout(timeout)
      }
      clearTimeoutsRef.current.clear()
      // Unlisten
      unlistenPromise.then(unlisten => {
        if (unlisten) unlisten()
      })
    }
  }, [dispatch])
}

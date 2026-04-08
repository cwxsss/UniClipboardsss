import { useEffect } from 'react'
import { daemonWs } from '@/lib/daemon-ws'
import { useAppDispatch } from '@/store/hooks'
import {
  markTransferFailed,
  cancelClipboardWrite,
  setEntryTransferStatus,
} from '@/store/slices/fileTransferSlice'

interface FileTransferStatusEvent {
  transferId: string
  entryId: string
  status: string
  reason?: string | null
}

/**
 * Hook that listens to file-transfer and clipboard events from the daemon WS,
 * dispatching durable status changes to the Redux fileTransfer slice.
 *
 * - `file-transfer.status_changed` events update entry transfer status.
 * - `clipboard.new_content` events cancel pending clipboard auto-writes.
 *
 * Call once in a top-level component (e.g. ClipboardContent) to activate.
 */
export function useTransferProgress(): void {
  const dispatch = useAppDispatch()

  useEffect(() => {
    let cancelled = false

    const handler = (event: { eventType: string; payload: unknown }) => {
      if (cancelled) return

      if (event.eventType === 'clipboard.new_content') {
        dispatch(cancelClipboardWrite())
        return
      }

      if (event.eventType === 'file-transfer.status_changed') {
        const payload = event.payload as FileTransferStatusEvent
        const { entryId, status, reason } = payload
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

        if (status === 'failed') {
          dispatch(
            markTransferFailed({
              transferId: payload.transferId,
              error: reason ?? undefined,
            })
          )
        }
      }
    }

    const unsubscribe = daemonWs.subscribe(['file-transfer', 'clipboard'], handler)

    return () => {
      cancelled = true
      unsubscribe()
    }
  }, [dispatch])
}

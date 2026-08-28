import { getCurrentWindow } from '@tauri-apps/api/window'
import type { PointerEvent } from 'react'
import { useCallback, useMemo, useRef } from 'react'
import { usePlatform } from '@/hooks/usePlatform'
import { createLogger } from '@/lib/logger'

const log = createLogger('window-drag')
const DRAG_THRESHOLD_PX = 4

function isDragExcludedTarget(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest('[data-tauri-drag-region="false"]') !== null
}

interface UseWindowDragOptions {
  enabled?: boolean
}

/** Start native window dragging after a small pointer movement threshold. */
export function useWindowDrag({ enabled = true }: UseWindowDragOptions = {}) {
  const { isTauri } = usePlatform()
  const windowRef = useMemo(
    () => (enabled && isTauri ? getCurrentWindow() : null),
    [enabled, isTauri]
  )
  const dragStartRef = useRef<{ x: number; y: number } | null>(null)

  const handlePointerDownCapture = useCallback(
    (event: PointerEvent<HTMLElement>) => {
      if (!enabled || !isTauri || event.button !== 0 || isDragExcludedTarget(event.target)) {
        dragStartRef.current = null
        return
      }

      dragStartRef.current = { x: event.clientX, y: event.clientY }
    },
    [enabled, isTauri]
  )

  const handlePointerMoveCapture = useCallback(
    (event: PointerEvent<HTMLElement>) => {
      const start = dragStartRef.current
      if (!start || (event.buttons & 1) === 0 || !windowRef) return
      if (Math.hypot(event.clientX - start.x, event.clientY - start.y) < DRAG_THRESHOLD_PX) {
        return
      }

      dragStartRef.current = null
      void windowRef.startDragging().catch(error => {
        log.error({ err: error }, 'Failed to start window dragging')
      })
    },
    [windowRef]
  )

  const clearDragStart = useCallback(() => {
    dragStartRef.current = null
  }, [])

  return {
    onPointerDownCapture: handlePointerDownCapture,
    onPointerMoveCapture: handlePointerMoveCapture,
    onPointerUpCapture: clearDragStart,
    onPointerCancelCapture: clearDragStart,
  }
}

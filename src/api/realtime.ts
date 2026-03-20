import { listen } from '@tauri-apps/api/event'

export const DAEMON_REALTIME_EVENT = 'daemon://realtime'

export interface DaemonRealtimeEnvelope<TPayload = unknown> {
  topic: string
  type: string
  ts: number
  payload: TPayload
}

export type FrontendRealtimeEvent = DaemonRealtimeEnvelope

export async function onDaemonRealtimeEvent(
  callback: (event: FrontendRealtimeEvent) => void
): Promise<() => void> {
  try {
    const unlisten = await listen<FrontendRealtimeEvent>(DAEMON_REALTIME_EVENT, event => {
      callback(event.payload)
    })

    return () => {
      unlisten()
    }
  } catch (error) {
    console.error('Failed to setup daemon realtime listener:', error)
    return () => {}
  }
}

import { useEffect, useState } from 'react'
import { daemonWs } from '@/lib/daemon-ws'
import { getEncryptionState } from '@/api/daemon'

interface EncryptionSessionState {
  encryptionReady: boolean
  isLocked: boolean
}

export function useEncryptionSessionState(): EncryptionSessionState {
  const [state, setState] = useState<EncryptionSessionState>({
    encryptionReady: false,
    isLocked: false,
  })

  useEffect(() => {
    let cancelled = false

    const syncState = async () => {
      try {
        const status = await getEncryptionState()
        if (cancelled) return

        const ready = !status.initialized || status.sessionReady
        setState({
          encryptionReady: ready,
          isLocked: status.initialized && !status.sessionReady,
        })
      } catch (err) {
        if (cancelled) return
        console.error('Failed to check encryption session status:', err)
        setState({ encryptionReady: true, isLocked: false })
      }
    }

    const handler = (event: { eventType: string }) => {
      if (cancelled) return
      if (event.eventType === 'encryption.sessionReady') {
        setState({ encryptionReady: true, isLocked: false })
      }
    }

    const unsubscribe = daemonWs.subscribe(['encryption'], handler)

    void syncState()

    return () => {
      cancelled = true
      unsubscribe()
    }
  }, [])

  return state
}

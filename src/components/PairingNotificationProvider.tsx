import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  acceptP2PPairing,
  classifyPairingError,
  onP2PPairingVerification,
  onSpaceAccessCompleted,
  rejectP2PPairing,
} from '@/api/p2p'
import PairingPinDialog from '@/components/PairingPinDialog'

export function PairingNotificationProvider() {
  const { t } = useTranslation()
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
  const activeSessionIdRef = useRef(activeSessionId)

  const [dialogState, setDialogState] = useState<{
    open: boolean
    pinCode: string
    peerDeviceName?: string
    peerId?: string
    phase?: 'display' | 'verifying' | 'success'
  }>({
    open: false,
    pinCode: '',
  })

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId
  }, [activeSessionId])

  const localizePairingError = useCallback(
    (error?: string | null) => {
      switch (classifyPairingError(error)) {
        case 'active_session_exists':
          return t('pairing.failed.errors.activeSession')
        case 'no_local_participant':
          return t('pairing.failed.errors.noParticipant')
        case 'session_not_found':
          return t('pairing.failed.errors.sessionExpired')
        case 'daemon_unavailable':
          return t('pairing.failed.errors.daemonUnavailable')
        default:
          return error || t('pairing.failed.title', { defaultValue: 'Pairing failed' })
      }
    },
    [t]
  )

  useEffect(() => {
    const unlistenVerificationPromise = onP2PPairingVerification(event => {
      const currentSessionId = activeSessionIdRef.current

      if (event.kind === 'request') {
        toast(
          t('pairing.request.title', {
            defaultValue: 'Pairing Request',
            device: event.deviceName || 'Unknown Device',
          }),
          {
            description: t('pairing.request.description', {
              defaultValue: 'A device wants to pair with you',
            }),
            action: {
              label: t('common.accept', { defaultValue: 'Accept' }),
              onClick: () => {
                activeSessionIdRef.current = event.sessionId
                setActiveSessionId(event.sessionId)
                acceptP2PPairing(event.sessionId).catch(err => {
                  const message = localizePairingError(
                    err instanceof Error ? err.message : String(err)
                  )
                  console.error('Failed to accept pairing request:', err)
                  toast.error(t('pairing.failed.title', { defaultValue: 'Pairing failed' }), {
                    description: message,
                  })
                  activeSessionIdRef.current = null
                  setActiveSessionId(null)
                })
              },
            },
            cancel: {
              label: t('common.reject', { defaultValue: 'Reject' }),
              onClick: () => {
                if (event.peerId) {
                  rejectP2PPairing(event.sessionId, event.peerId).catch(err => {
                    const message = localizePairingError(
                      err instanceof Error ? err.message : String(err)
                    )
                    console.error('Failed to reject pairing request:', err)
                    toast.error(t('pairing.failed.title', { defaultValue: 'Pairing failed' }), {
                      description: message,
                    })
                  })
                }
              },
            },
            duration: 30_000,
          }
        )
        return
      }

      if (!currentSessionId || event.sessionId !== currentSessionId) {
        return
      }

      if (event.kind === 'verification' && event.code) {
        setDialogState({
          open: true,
          pinCode: event.code,
          peerDeviceName: event.deviceName,
          peerId: event.peerId,
          phase: 'display',
        })
        return
      }

      if (event.kind === 'verifying' || event.kind === 'complete') {
        setDialogState(prev => ({ ...prev, phase: 'verifying' }))
        return
      }

      if (event.kind === 'failed') {
        setDialogState(prev => ({ ...prev, open: false }))
        toast.error(t('pairing.failed.title', { defaultValue: 'Pairing failed' }), {
          description: localizePairingError(event.error),
        })
        setActiveSessionId(null)
      }
    })

    const unlistenSpaceAccessPromise = onSpaceAccessCompleted(event => {
      const currentSessionId = activeSessionIdRef.current
      if (!currentSessionId || event.sessionId !== currentSessionId) {
        return
      }

      if (event.success) {
        setDialogState(prev => ({ ...prev, phase: 'success' }))
        setTimeout(() => {
          setDialogState(prev => ({ ...prev, open: false }))
          setActiveSessionId(null)
        }, 2000)
        return
      }

      setDialogState(prev => ({ ...prev, open: false }))
      toast.error(t('pairing.failed.title', { defaultValue: 'Pairing failed' }), {
        description: localizePairingError(event.reason),
      })
      setActiveSessionId(null)
    })

    return () => {
      unlistenVerificationPromise.then(unlisten => unlisten())
      unlistenSpaceAccessPromise.then(unlisten => unlisten())
    }
  }, [localizePairingError, t])

  const handleCancel = () => {
    if (activeSessionIdRef.current && dialogState.peerId) {
      rejectP2PPairing(activeSessionIdRef.current, dialogState.peerId).catch(err => {
        const message = localizePairingError(err instanceof Error ? err.message : String(err))
        console.error('Failed to cancel pairing dialog:', err)
        toast.error(t('pairing.failed.title', { defaultValue: 'Pairing failed' }), {
          description: message,
        })
      })
    }
    setDialogState(prev => ({ ...prev, open: false }))
    setActiveSessionId(null)
  }

  return (
    <PairingPinDialog
      open={dialogState.open}
      onClose={handleCancel}
      pinCode={dialogState.pinCode}
      peerDeviceName={dialogState.peerDeviceName}
      isInitiator={false}
      onConfirm={matches => {
        if (!matches) {
          handleCancel()
        }
      }}
      phase={dialogState.phase}
    />
  )
}

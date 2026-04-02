import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { classifyPairingError } from '@/api/daemon/events'
import { acceptP2PPairing, rejectP2PPairing } from '@/api/daemon/pairing'
import { usePairingEvents } from '@/hooks/useDaemonEvents'
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

  usePairingEvents({
    onRequest: ({ sessionId, peerId, deviceName }) => {
      toast(
        t('pairing.request.title', {
          defaultValue: 'Pairing Request',
          device: deviceName || 'Unknown Device',
        }),
        {
          description: t('pairing.request.description', {
            defaultValue: 'A device wants to pair with you',
          }),
          action: {
            label: t('common.accept', { defaultValue: 'Accept' }),
            onClick: () => {
              activeSessionIdRef.current = sessionId
              setActiveSessionId(sessionId)
              acceptP2PPairing(sessionId).catch(err => {
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
              if (peerId) {
                rejectP2PPairing(sessionId, peerId).catch(err => {
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
    },

    onVerification: ({ sessionId, deviceName, code, peerId }) => {
      const currentSessionId = activeSessionIdRef.current
      if (!currentSessionId || sessionId !== currentSessionId) return

      setDialogState({
        open: true,
        pinCode: code ?? '',
        peerDeviceName: deviceName,
        peerId: peerId,
        phase: 'display',
      })
    },

    onVerifying: () => {
      setDialogState(prev => ({ ...prev, phase: 'verifying' }))
    },

    onComplete: () => {
      setDialogState(prev => ({ ...prev, phase: 'verifying' }))
    },

    onFailed: ({ sessionId, error }) => {
      const currentSessionId = activeSessionIdRef.current
      if (!currentSessionId || sessionId !== currentSessionId) return
      setDialogState(prev => ({ ...prev, open: false }))
      toast.error(t('pairing.failed.title', { defaultValue: 'Pairing failed' }), {
        description: localizePairingError(error),
      })
      setActiveSessionId(null)
    },

    onSpaceAccessCompleted: ({ sessionId, success, reason }) => {
      const currentSessionId = activeSessionIdRef.current
      if (!currentSessionId || sessionId !== currentSessionId) return

      if (success) {
        setDialogState(prev => ({ ...prev, phase: 'success' }))
        setTimeout(() => {
          setDialogState(prev => ({ ...prev, open: false }))
          setActiveSessionId(null)
        }, 2000)
        return
      }

      setDialogState(prev => ({ ...prev, open: false }))
      toast.error(t('pairing.failed.title', { defaultValue: 'Pairing failed' }), {
        description: localizePairingError(reason),
      })
      setActiveSessionId(null)
    },
  })

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

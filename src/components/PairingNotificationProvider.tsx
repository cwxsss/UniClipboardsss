import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  acceptP2PPairing,
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
                  console.error(err)
                  toast.error(t('pairing.failed', { defaultValue: 'Pairing failed' }))
                  activeSessionIdRef.current = null
                  setActiveSessionId(null)
                })
              },
            },
            cancel: {
              label: t('common.reject', { defaultValue: 'Reject' }),
              onClick: () => {
                if (event.peerId) {
                  rejectP2PPairing(event.sessionId, event.peerId).catch(console.error)
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
        toast.error(event.error || t('pairing.failed', { defaultValue: 'Pairing failed' }))
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
      toast.error(event.reason || t('pairing.failed', { defaultValue: 'Pairing failed' }))
      setActiveSessionId(null)
    })

    return () => {
      unlistenVerificationPromise.then(unlisten => unlisten())
      unlistenSpaceAccessPromise.then(unlisten => unlisten())
    }
  }, [t])

  const handleCancel = () => {
    if (activeSessionIdRef.current && dialogState.peerId) {
      rejectP2PPairing(activeSessionIdRef.current, dialogState.peerId).catch(console.error)
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

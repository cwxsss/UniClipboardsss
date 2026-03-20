import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { acceptP2PPairing, rejectP2PPairing } from '@/api/p2p'
import { onDaemonRealtimeEvent } from '@/api/realtime'
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
    const unlistenPromise = onDaemonRealtimeEvent(event => {
      const currentSessionId = activeSessionIdRef.current

      if (event.topic === 'setup' && event.type === 'setup.spaceAccessCompleted') {
        const payload = event.payload as {
          sessionId: string
          success: boolean
          reason?: string
        }

        if (currentSessionId && payload.sessionId === currentSessionId) {
          if (payload.success) {
            setDialogState(prev => ({ ...prev, phase: 'success' }))
            setTimeout(() => {
              setDialogState(prev => ({ ...prev, open: false }))
              setActiveSessionId(null)
            }, 2000)
          } else {
            setDialogState(prev => ({ ...prev, open: false }))
            toast.error(payload.reason || t('pairing.failed', { defaultValue: 'Pairing failed' }))
            setActiveSessionId(null)
          }
        }
        return
      }

      if (event.topic !== 'pairing') {
        return
      }

      if (event.type === 'pairing.updated') {
        const payload = event.payload as {
          sessionId: string
          status: string
          peerId?: string
          deviceName?: string
        }

        if (payload.status !== 'request') {
          if (payload.status === 'verifying' && currentSessionId === payload.sessionId) {
            setDialogState(prev => ({ ...prev, phase: 'verifying' }))
          }
          return
        }

        toast(
          t('pairing.request.title', {
            defaultValue: 'Pairing Request',
            device: payload.deviceName || 'Unknown Device',
          }),
          {
            description: t('pairing.request.description', {
              defaultValue: 'A device wants to pair with you',
            }),
            action: {
              label: t('common.accept', { defaultValue: 'Accept' }),
              onClick: () => {
                activeSessionIdRef.current = payload.sessionId
                setActiveSessionId(payload.sessionId)
                acceptP2PPairing(payload.sessionId).catch(err => {
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
                if (payload.peerId) {
                  rejectP2PPairing(payload.sessionId, payload.peerId).catch(console.error)
                }
              },
            },
            duration: 30_000,
          }
        )
        return
      }

      const payload = event.payload as {
        sessionId: string
        peerId?: string
        deviceName?: string
        code?: string
        error?: string
      }

      if (currentSessionId && payload.sessionId === currentSessionId) {
        if (event.type === 'pairing.verificationRequired') {
          if (payload.code) {
            setDialogState({
              open: true,
              pinCode: payload.code,
              peerDeviceName: payload.deviceName,
              peerId: payload.peerId,
              phase: 'display',
            })
          }
        } else if (event.type === 'pairing.complete') {
          setDialogState(prev => ({ ...prev, phase: 'verifying' }))
        } else if (event.type === 'pairing.failed') {
          setDialogState(prev => ({ ...prev, open: false }))
          toast.error(payload.error || t('pairing.failed', { defaultValue: 'Pairing failed' }))
          setActiveSessionId(null)
        }
      }
    })

    return () => {
      unlistenPromise.then(unlisten => unlisten())
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

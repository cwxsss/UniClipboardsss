import { AnimatePresence, motion } from 'framer-motion'
import { Loader2, Monitor, Shield, Smartphone, Wifi, Key } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { toast } from 'sonner'
import { getP2PPeers, P2PPeerInfo } from '@/api/p2p'
import {
  cancelSetup,
  getSetupState,
  confirmPeerTrust,
  onSetupStateChanged,
  selectJoinPeer,
  startJoinSpace,
  startNewSpace,
  submitPassphrase,
  verifyPassphrase,
  SetupState,
} from '@/api/setup'
import { Button } from '@/components/ui/button'
import CreatePassphraseStep from '@/pages/setup/CreatePassphraseStep'
import JoinPickDeviceStep from '@/pages/setup/JoinPickDeviceStep'
import JoinVerifyPassphraseStep from '@/pages/setup/JoinVerifyPassphraseStep'
import PairingConfirmStep from '@/pages/setup/PairingConfirmStep'
import SetupDoneStep from '@/pages/setup/SetupDoneStep'
import WelcomeStep from '@/pages/setup/WelcomeStep'

type SetupPageProps = {
  onCompleteSetup?: () => void
}

export default function SetupPage({ onCompleteSetup }: SetupPageProps = {}) {
  const { t } = useTranslation(undefined, { keyPrefix: 'setup.page' })
  const { t: tCommon } = useTranslation(undefined, { keyPrefix: 'setup.common' })
  const navigate = useNavigate()
  const [setupState, setSetupState] = useState<SetupState | null>(null)
  const [loading, setLoading] = useState(false)
  const [peers, setPeers] = useState<Array<{ id: string; name: string; device_type: string }>>([])
  const [peersLoading, setPeersLoading] = useState(false)
  const [isScanningInitial, setIsScanningInitial] = useState(true)
  const [selectedPeerId, setSelectedPeerId] = useState<string | null>(null)
  const activeEventSessionIdRef = useRef<string | null>(null)
  const setupStateRef = useRef<SetupState | null>(null)

  const isSetupFlowActive = useCallback((state: SetupState | null) => {
    if (!state) return false
    return state !== 'Welcome' && state !== 'Completed'
  }, [])

  const syncSetupState = useCallback((nextState: SetupState) => {
    setSetupState(prevState => {
      if (JSON.stringify(prevState) === JSON.stringify(nextState)) {
        return prevState
      }
      return nextState
    })
  }, [])

  useEffect(() => {
    const loadState = async () => {
      try {
        const state = await getSetupState()
        syncSetupState(state)
      } catch (error) {
        console.error('Failed to load setup state:', error)
        toast.error(t('errors.loadSetupStateFailed'))
      }
    }
    loadState()
  }, [syncSetupState, t])

  useEffect(() => {
    setupStateRef.current = setupState
  }, [setupState])

  useEffect(() => {
    let mounted = true
    let unlisten: (() => void) | null = null

    const setupListener = async () => {
      unlisten = await onSetupStateChanged(event => {
        if (!mounted) {
          return
        }

        if (!isSetupFlowActive(setupStateRef.current)) {
          return
        }

        if (!activeEventSessionIdRef.current) {
          activeEventSessionIdRef.current = event.sessionId
        }

        if (activeEventSessionIdRef.current !== event.sessionId) {
          return
        }

        syncSetupState(event.state)

        if (event.state === 'Completed' || event.state === 'Welcome') {
          activeEventSessionIdRef.current = null
        }
      })
    }

    setupListener()

    return () => {
      mounted = false
      activeEventSessionIdRef.current = null
      if (unlisten) {
        unlisten()
      }
    }
  }, [isSetupFlowActive, syncSetupState])

  useEffect(() => {
    if (!isSetupFlowActive(setupState)) {
      activeEventSessionIdRef.current = null
    }
  }, [isSetupFlowActive, setupState])

  const handleRefreshPeers = useCallback(async () => {
    setPeersLoading(true)
    try {
      const peerList = await getP2PPeers()
      setPeers(
        peerList.map((p: P2PPeerInfo) => ({
          id: p.peerId,
          name: p.deviceName || tCommon('unknownDevice'),
          device_type: 'desktop',
        }))
      )
    } catch (error) {
      console.error('Failed to refresh peers:', error)
      toast.error(t('errors.refreshPeersFailed'))
    } finally {
      setPeersLoading(false)
      setIsScanningInitial(false)
    }
  }, [t, tCommon])

  useEffect(() => {
    if (setupState && typeof setupState === 'object' && 'JoinSpaceSelectDevice' in setupState) {
      handleRefreshPeers()
      const interval = setInterval(handleRefreshPeers, 3000)
      return () => {
        clearInterval(interval)
      }
    } else {
      setIsScanningInitial(true)
    }
  }, [setupState, handleRefreshPeers])

  const runAction = async (action: () => Promise<SetupState>) => {
    setLoading(true)
    try {
      const newState = await action()
      syncSetupState(newState)
    } catch (error) {
      console.error('Failed to dispatch event:', error)
      toast.error(t('errors.operationFailed'))
    } finally {
      setLoading(false)
    }
  }

  const renderStep = () => {
    if (!setupState) {
      return (
        <div className="flex h-full w-full items-center justify-center">
          <div className="flex items-center gap-3 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('loadingSetupState')}
          </div>
        </div>
      )
    }

    if (setupState === 'Welcome') {
      return (
        <WelcomeStep
          onCreate={() => runAction(() => startNewSpace())}
          onJoin={() => runAction(() => startJoinSpace())}
          loading={loading}
        />
      )
    }

    if (setupState === 'Completed') {
      return (
        <SetupDoneStep
          onComplete={() => {
            onCompleteSetup?.()
            navigate('/', { replace: true })
          }}
          loading={loading}
        />
      )
    }

    if (typeof setupState === 'object') {
      if ('CreateSpaceInputPassphrase' in setupState) {
        return (
          <CreatePassphraseStep
            onSubmit={(pass1: string, pass2: string) =>
              runAction(() => submitPassphrase(pass1, pass2))
            }
            onBack={() => runAction(() => cancelSetup())}
            error={setupState.CreateSpaceInputPassphrase.error}
            loading={loading}
          />
        )
      }

      if ('JoinSpaceSelectDevice' in setupState) {
        return (
          <JoinPickDeviceStep
            onSelectPeer={(peerId: string) => {
              setSelectedPeerId(peerId)
              runAction(() => selectJoinPeer(peerId))
            }}
            onBack={() => runAction(() => cancelSetup())}
            onRefresh={handleRefreshPeers}
            peers={peers}
            error={setupState.JoinSpaceSelectDevice.error}
            loading={loading || peersLoading}
            isScanningInitial={isScanningInitial}
          />
        )
      }

      if ('JoinSpaceInputPassphrase' in setupState) {
        const { error } = setupState.JoinSpaceInputPassphrase
        return (
          <JoinVerifyPassphraseStep
            peerId={selectedPeerId ?? undefined}
            onSubmit={(passphrase: string) => runAction(() => verifyPassphrase(passphrase))}
            onBack={() => runAction(() => cancelSetup())}
            onCreateNew={() => runAction(() => startNewSpace())}
            error={error}
            loading={loading}
          />
        )
      }

      if ('JoinSpaceConfirmPeer' in setupState) {
        const { short_code, peer_fingerprint, error } = setupState.JoinSpaceConfirmPeer
        return (
          <PairingConfirmStep
            shortCode={short_code}
            peerFingerprint={peer_fingerprint}
            onConfirm={() => runAction(() => confirmPeerTrust())}
            onCancel={() => runAction(() => cancelSetup())}
            error={error}
            loading={loading}
          />
        )
      }

      if ('ProcessingCreateSpace' in setupState) {
        const message = setupState.ProcessingCreateSpace.message
        return (
          <div className="flex h-full w-full items-center justify-center">
            <div className="flex items-center gap-3 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              {message ?? t('processing')}
            </div>
          </div>
        )
      }

      if ('ProcessingJoinSpace' in setupState) {
        return (
          <motion.div
            initial={{ opacity: 0, scale: 0.98 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.98 }}
            className="w-full"
          >
            <div className="flex flex-col items-center justify-center py-12 text-center">
              <div className="mb-6 flex h-16 w-16 items-center justify-center rounded-full bg-primary/10">
                <Loader2 className="h-8 w-8 animate-spin text-primary" />
              </div>
              <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                {t('processingJoinSpace.title')}
              </h1>
              <p className="mt-2 max-w-sm text-muted-foreground">
                {t('processingJoinSpace.subtitle')}
              </p>
              <div className="mt-8 flex items-center gap-2.5 rounded-lg border border-border/50 bg-muted/40 px-5 py-3 text-sm text-muted-foreground">
                <div className="flex shrink-0 items-center gap-1">
                  <Monitor className="h-4 w-4" />
                  <span className="text-xs">/</span>
                  <Smartphone className="h-4 w-4" />
                </div>
                <span>{t('processingJoinSpace.hint')}</span>
              </div>
              <Button
                variant="ghost"
                className="mt-8 text-muted-foreground"
                onClick={() => runAction(() => cancelSetup())}
                disabled={loading}
              >
                {tCommon('cancel')}
              </Button>
            </div>
          </motion.div>
        )
      }
    }

    return <div>{t('unknownState', { state: JSON.stringify(setupState) })}</div>
  }

  const stepKey = useMemo(() => {
    if (!setupState) return 'loading'
    if (typeof setupState === 'string') return setupState
    return Object.keys(setupState)[0] ?? 'unknown'
  }, [setupState])

  return (
    <div className="relative h-full w-full overflow-hidden bg-background">
      <div className="pointer-events-none absolute inset-0">
        <div className="absolute inset-0 bg-gradient-to-br from-background via-background to-muted/20" />
        <div className="absolute -top-32 -left-32 h-96 w-96 bg-primary/5 blur-3xl" />
        <div className="absolute -bottom-32 -right-32 h-96 w-96 bg-emerald-500/5 blur-3xl" />
      </div>

      <div className="relative flex h-full w-full flex-col">
        <main className="flex flex-1 items-center overflow-y-auto px-6 py-12 lg:px-16">
          <div className="mx-auto w-full max-w-2xl">
            <AnimatePresence mode="wait" initial={false}>
              <div key={stepKey}>{renderStep()}</div>
            </AnimatePresence>
          </div>
        </main>

        <div className="pointer-events-none absolute bottom-6 right-6 hidden flex-col gap-2 text-[0.625rem] text-muted-foreground/60 lg:flex">
          <div className="flex items-center gap-1.5">
            <Shield className="h-3 w-3" />
            <span>{t('badges.e2ee')}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <Key className="h-3 w-3" />
            <span>{t('badges.localKeys')}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <Wifi className="h-3 w-3" />
            <span>{t('badges.lanDiscovery')}</span>
          </div>
        </div>
      </div>
    </div>
  )
}

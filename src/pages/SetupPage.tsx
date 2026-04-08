import { AnimatePresence } from 'framer-motion'
import { ArrowLeft, Loader2 } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router-dom'
import { useSelector } from 'react-redux'
import {
  cancelSetup,
  confirmPeerTrust,
  selectJoinPeer,
  startJoinSpace,
  startNewSpace,
  submitPassphrase,
  verifyPassphrase,
} from '@/api/daemon/setup'
import FloatingParticles from '@/components/effects/FloatingParticles'
import { useDeviceDiscovery } from '@/hooks/useDeviceDiscovery'
import { usePlatform } from '@/hooks/usePlatform'
import { useSetupFlow } from '@/hooks/useSetupFlow'
import CreatePassphraseStep from '@/pages/setup/CreatePassphraseStep'
import JoinPickDeviceStep from '@/pages/setup/JoinPickDeviceStep'
import JoinVerifyPassphraseStep from '@/pages/setup/JoinVerifyPassphraseStep'
import PairingConfirmStep from '@/pages/setup/PairingConfirmStep'
import ProcessingJoinStep from '@/pages/setup/ProcessingJoinStep'
import SetupDoneStep from '@/pages/setup/SetupDoneStep'
import StepDotIndicator from '@/pages/setup/StepDotIndicator'
import WelcomeStep from '@/pages/setup/WelcomeStep'
import type { RootState } from '@/store'

type SetupPageProps = {
  onCompleteSetup?: () => void
}

export default function SetupPage({ onCompleteSetup }: SetupPageProps = {}) {
  const { t } = useTranslation(undefined, { keyPrefix: 'setup.page' })
  const { t: tCommon } = useTranslation(undefined, { keyPrefix: 'setup.common' })
  const { isMac } = usePlatform()
  const navigate = useNavigate()

  const {
    setupState,
    hydrated,
    stepInfo,
    direction,
    loading,
    runAction,
    selectedPeerId,
    setSelectedPeerId,
  } = useSetupFlow()

  // discoveredPeers from Redux (populated by useDeviceDiscovery via daemon events)
  const discoveredPeers = useSelector((state: RootState) => state.devices.discoveredPeers)

  const isJoinSelectActive =
    !!setupState && typeof setupState === 'object' && 'JoinSpaceSelectDevice' in setupState

  const { scanPhase, resetScan } = useDeviceDiscovery(isJoinSelectActive, {
    onError: () => {
      // Error toast handled by useDeviceDiscovery internally
    },
  })

  const renderStep = () => {
    if (!hydrated || !setupState) {
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
          direction={direction}
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
          direction={direction}
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
            error={setupState.CreateSpaceInputPassphrase.error}
            loading={loading}
            direction={direction}
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
            onRescan={resetScan}
            peers={discoveredPeers}
            scanPhase={scanPhase}
            error={setupState.JoinSpaceSelectDevice.error}
            loading={loading}
            direction={direction}
          />
        )
      }

      if ('JoinSpaceInputPassphrase' in setupState) {
        const { error } = setupState.JoinSpaceInputPassphrase
        return (
          <JoinVerifyPassphraseStep
            peerId={selectedPeerId ?? undefined}
            onSubmit={(passphrase: string) => runAction(() => verifyPassphrase(passphrase))}
            onCreateNew={() => runAction(() => startNewSpace())}
            error={error}
            loading={loading}
            direction={direction}
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
            direction={direction}
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
          <ProcessingJoinStep
            onCancel={() => runAction(() => cancelSetup())}
            loading={loading}
            direction={direction}
          />
        )
      }
    }

    return (
      <div className="break-all text-sm text-muted-foreground">
        {t('unknownState', { state: JSON.stringify(setupState) })}
      </div>
    )
  }

  const stepKey = useMemo(() => {
    if (!setupState) return 'loading'
    if (typeof setupState === 'string') return setupState
    return Object.keys(setupState)[0] ?? 'unknown'
  }, [setupState])

  return (
    <div className="relative h-full w-full overflow-hidden bg-background">
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        <div className="absolute inset-0 bg-gradient-to-br from-background via-background to-muted/30" />
        <div
          className="absolute -top-32 -left-32 h-[28rem] w-[28rem] rounded-full bg-blue-500/25 blur-[6rem] dark:bg-blue-500/15"
          style={{ animation: 'aurora-drift-1 12s ease-in-out infinite' }}
        />
        <div
          className="absolute -bottom-24 -right-24 h-[24rem] w-[24rem] rounded-full bg-emerald-500/25 blur-[5rem] dark:bg-emerald-500/15"
          style={{ animation: 'aurora-drift-2 15s ease-in-out infinite' }}
        />
        <div
          className="absolute top-1/3 left-1/2 h-[20rem] w-[20rem] -translate-x-1/2 rounded-full bg-violet-500/20 blur-[5rem] dark:bg-violet-500/12"
          style={{ animation: 'aurora-drift-3 18s ease-in-out infinite' }}
        />
        <FloatingParticles />
      </div>

      <div className="relative flex h-full w-full min-h-0 flex-col">
        {/* Draggable header with back button */}
        <header
          data-tauri-drag-region
          className={`relative z-10 flex h-12 shrink-0 items-center pr-4 ${
            isMac ? 'pl-20' : 'pl-4'
          }`}
        >
          {setupState &&
            typeof setupState === 'object' &&
            ('CreateSpaceInputPassphrase' in setupState ||
              'JoinSpaceSelectDevice' in setupState ||
              'JoinSpaceInputPassphrase' in setupState) && (
              <button
                type="button"
                data-tauri-drag-region="false"
                onClick={() => runAction(() => cancelSetup())}
                className="flex items-center gap-1 text-sm text-muted-foreground transition-colors hover:text-foreground"
              >
                <ArrowLeft className="h-4 w-4" />
                {tCommon('back')}
              </button>
            )}
        </header>

        <main
          className={`flex min-h-0 flex-1 items-center px-8 py-4 sm:px-12 sm:py-6 ${
            stepKey === 'Welcome' ? 'overflow-hidden' : 'overflow-y-auto'
          }`}
        >
          <div className="mx-auto w-full max-w-3xl max-h-full">
            <div className="max-h-full px-1 py-1 sm:px-0 sm:py-2">
              <AnimatePresence mode="wait" initial={false}>
                <div key={stepKey} className="w-full">
                  {renderStep()}
                </div>
              </AnimatePresence>
            </div>
          </div>
        </main>

        {stepInfo && (
          <div className="flex justify-center pb-4">
            <StepDotIndicator totalSteps={stepInfo.total} currentStep={stepInfo.current} />
          </div>
        )}
      </div>
    </div>
  )
}

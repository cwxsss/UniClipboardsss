import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { DaemonApiError, DaemonErrorCode } from '@/api/daemon/errors'
import { getRelayCredentialStatus, probeRelayUrl } from '@/api/daemon/settings'
import type { RelayCredentialEdit, RelaySaveContextResult } from '@/types/setting'
import { canonicalRelayUrl, outcomeToStatus, type ProbeStatus } from './relay-probe'
export interface RelayEditorOptions {
  initialUrl: string
  onSave: (url: string, credential: RelayCredentialEdit) => Promise<RelaySaveContextResult>
  onRemove: () => void | Promise<void>
}

const IDLE: ProbeStatus = { kind: 'idle' }

/**
 * Distinguish "the relay address is wrong" from "the sync engine is down".
 *
 * The daemon maps `EngineErrorCategory::Unavailable` (which is what an aborted
 * engine startup produces — e.g. rc.17's fail-closed pre-upgrade profile backup)
 * to HTTP 503, while a bad/duplicate address or token is a 400. Those two need
 * different copy: a 400 tells the user to fix the address, a 503 tells them the
 * address is NOT the problem and to look at the daemon instead.
 *
 * 区分"中继地址有问题"与"同步引擎没起来"：daemon 把引擎不可用映射为 503，
 * 地址/令牌非法映射为 400，两者需要完全不同的提示文案。
 */
function isEngineUnavailableError(err: unknown): boolean {
  return err instanceof DaemonApiError && err.code === DaemonErrorCode.SERVICE_UNAVAILABLE
}

function errorMessageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

export function useRelayEditor({ initialUrl, onSave, onRemove }: RelayEditorOptions) {
  const { t } = useTranslation()
  const [url, setUrl] = useState(initialUrl)
  const [accessToken, setAccessToken] = useState('')
  const [configured, setConfigured] = useState<boolean | null>(initialUrl ? null : false)
  const [removeSavedToken, setRemoveSavedToken] = useState(false)
  const [visible, setVisible] = useState(false)
  const [probeStatus, setProbeStatus] = useState<ProbeStatus>(IDLE)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const probeGenerationRef = useRef(0)
  const trimmedUrl = url.trim()
  const canonicalUrl = canonicalRelayUrl(trimmedUrl)
  const canonicalInitialUrl = canonicalRelayUrl(initialUrl)
  const hasUrlChanged =
    canonicalUrl === null || canonicalInitialUrl === null || canonicalUrl !== canonicalInitialUrl
  const waitingForCredentialStatus =
    Boolean(initialUrl) &&
    !hasUrlChanged &&
    !accessToken &&
    !removeSavedToken &&
    configured === null
  const canTest =
    trimmedUrl.length > 0 &&
    probeStatus.kind !== 'testing' &&
    !saving &&
    !waitingForCredentialStatus
  const canSave = probeStatus.kind === 'success' && !saving

  useEffect(() => {
    if (!initialUrl) return
    let current = true
    void getRelayCredentialStatus(initialUrl).then(
      status => {
        if (current) setConfigured(status.configured)
      },
      err => {
        if (!current) return
        setConfigured(false)
        setError(
          t('settings.sections.network.customRelays.credentials.loadError', {
            message: err instanceof Error ? err.message : String(err),
          })
        )
      }
    )
    return () => {
      current = false
    }
  }, [initialUrl, t])

  const resetProbe = () => {
    probeGenerationRef.current += 1
    setProbeStatus(IDLE)
    setError(null)
  }

  const updateUrl = (nextUrl: string) => {
    setUrl(nextUrl)
    if (nextUrl.trim() !== initialUrl) setRemoveSavedToken(false)
    resetProbe()
  }

  const updateAccessToken = (nextToken: string) => {
    setAccessToken(nextToken)
    resetProbe()
  }

  const isCurrentProbe = (current: ProbeStatus, generation: number, pendingUrl: string) =>
    probeGenerationRef.current === generation &&
    current.kind === 'testing' &&
    current.pendingUrl === pendingUrl

  const testAvailability = async () => {
    if (!canTest) return
    const generation = probeGenerationRef.current + 1
    probeGenerationRef.current = generation
    setError(null)
    setProbeStatus({ kind: 'testing', pendingUrl: trimmedUrl })
    try {
      const credential = accessToken
        ? { mode: 'override' as const, accessToken }
        : removeSavedToken || hasUrlChanged || configured !== true
          ? { mode: 'none' as const }
          : { mode: 'stored' as const }
      const outcome = await probeRelayUrl(trimmedUrl, credential)
      setProbeStatus(current => {
        if (!isCurrentProbe(current, generation, trimmedUrl)) return current
        return outcomeToStatus(outcome, t)
      })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setProbeStatus(current => {
        if (!isCurrentProbe(current, generation, trimmedUrl)) return current
        return {
          kind: 'failure',
          message: t('settings.sections.network.customRelays.testErrors.unavailable', {
            defaultValue: message,
          }),
        }
      })
    }
  }

  const saveRelay = async () => {
    if (!canSave) return
    setSaving(true)
    setError(null)
    try {
      const credential: RelayCredentialEdit = removeSavedToken
        ? { action: 'delete' }
        : accessToken
          ? { action: 'set', accessToken }
          : { action: 'keep' }
      const result = await onSave(trimmedUrl, credential)
      setConfigured(result.credentialStatus.configured)
      setAccessToken('')
      setVisible(false)
      setRemoveSavedToken(false)
    } catch (err) {
      const message = errorMessageOf(err)
      // A 503 here means the engine (or its credential store) never came up —
      // NOT that the address is bad. Say so, otherwise users keep editing an
      // address that was never the problem.
      setError(
        isEngineUnavailableError(err)
          ? t('settings.sections.network.customRelays.engineUnavailableError', { message })
          : t('settings.sections.network.customRelays.credentials.saveError', { message })
      )
    } finally {
      setSaving(false)
    }
  }

  const removeRelay = async () => {
    if (saving) return
    setSaving(true)
    setError(null)
    try {
      await onRemove()
    } catch (err) {
      const message = errorMessageOf(err)
      setError(
        isEngineUnavailableError(err)
          ? t('settings.sections.network.customRelays.engineUnavailableError', { message })
          : t('settings.sections.network.customRelays.saveError', { message })
      )
    } finally {
      setSaving(false)
    }
  }

  const toggleTokenRemoval = () => {
    setRemoveSavedToken(value => !value)
    setAccessToken('')
    resetProbe()
  }
  return {
    url,
    accessToken,
    configured,
    removeSavedToken,
    visible,
    probeStatus,
    saving,
    error,
    hasUrlChanged,
    canTest,
    canSave,
    updateUrl,
    updateAccessToken,
    testAvailability,
    saveRelay,
    removeRelay,
    toggleTokenRemoval,
    toggleVisible: () => setVisible(value => !value),
  }
}

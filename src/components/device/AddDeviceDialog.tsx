import { Copy, Loader2, RefreshCw, XCircle } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  cancelInvitation,
  getSetupState,
  issuePairingInvitation,
  type CurrentInvitation,
} from '@/api/daemon/setupV2'
import { formatInvitationCode } from '@/components/InvitationCodeInput'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { createLogger } from '@/lib/logger'
import { cn } from '@/lib/utils'

const log = createLogger('add-device-dialog')

interface AddDeviceDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

function formatRemaining(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000))
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
}

export default function AddDeviceDialog({ open, onOpenChange }: AddDeviceDialogProps) {
  const { t } = useTranslation()
  const [invitation, setInvitation] = useState<CurrentInvitation | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [now, setNow] = useState(() => Date.now())
  const [copied, setCopied] = useState(false)

  // 倒计时 tick — 仅在有邀请时启动
  useEffect(() => {
    if (!open || !invitation) return
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [open, invitation])

  // 打开时：优先恢复 currentInvitation，否则申请新邀请
  // 后端约束"同一时刻一个邀请"，关闭对话框不取消邀请，重开会拿回同一个
  useEffect(() => {
    if (!open) return
    let cancelled = false
    void (async () => {
      setLoading(true)
      setError(null)
      try {
        const state = await getSetupState()
        if (cancelled) return
        if (state.currentInvitation) {
          setInvitation(state.currentInvitation)
        } else {
          const issued = await issuePairingInvitation()
          if (cancelled) return
          setInvitation(issued)
        }
      } catch (err) {
        if (cancelled) return
        log.error({ err }, 'Failed to load or issue invitation')
        setError(t('devices.addDevice.errors.issueFailed'))
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [open, t])

  // 关闭时清状态，避免下次打开闪现旧值
  useEffect(() => {
    if (!open) {
      setInvitation(null)
      setError(null)
      setCopied(false)
    }
  }, [open])

  const remaining = invitation ? Math.max(0, invitation.expiresAtMs - now) : 0
  const expired = invitation ? remaining <= 0 : false
  const display = useMemo(
    () => (invitation ? formatInvitationCode(invitation.code) : ''),
    [invitation]
  )

  const handleCopy = async () => {
    if (!invitation) return
    try {
      await navigator.clipboard.writeText(invitation.code)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch (err) {
      log.warn({ err }, 'clipboard.writeText failed')
    }
  }

  const handleCancel = async () => {
    setLoading(true)
    try {
      await cancelInvitation()
    } catch (err) {
      // 邀请可能已被服务端清理，关闭即可
      log.warn({ err }, 'cancelInvitation failed (ignored on close)')
    } finally {
      setLoading(false)
      onOpenChange(false)
    }
  }

  const handleRegenerate = async () => {
    setLoading(true)
    setError(null)
    try {
      try {
        await cancelInvitation()
      } catch (err) {
        log.warn({ err }, 'cancelInvitation before regenerate failed')
      }
      const issued = await issuePairingInvitation()
      setInvitation(issued)
    } catch (err) {
      log.error({ err }, 'Regenerate invitation failed')
      setError(t('devices.addDevice.errors.issueFailed'))
    } finally {
      setLoading(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" onInteractOutside={e => e.preventDefault()}>
        <DialogHeader>
          <DialogTitle>{t('devices.addDevice.title')}</DialogTitle>
          <DialogDescription>{t('devices.addDevice.subtitle')}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col items-center gap-5 py-4">
          {loading && !invitation ? (
            <div className="flex items-center gap-3 py-8 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t('devices.addDevice.loading')}
            </div>
          ) : error && !invitation ? (
            <p className="py-8 text-sm text-destructive">{error}</p>
          ) : invitation ? (
            <>
              <div className="relative w-full max-w-xs">
                <div
                  className={cn(
                    'rounded-xl border border-border/50 bg-muted/30 px-6 py-5 text-center font-mono text-3xl font-semibold tracking-[0.4em] text-foreground sm:text-4xl',
                    expired && 'opacity-40'
                  )}
                >
                  {display}
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="absolute right-2 top-1/2 -translate-y-1/2"
                  onClick={handleCopy}
                  disabled={expired}
                  title={
                    copied
                      ? t('devices.addDevice.actions.copied')
                      : t('devices.addDevice.actions.copy')
                  }
                >
                  <Copy className="h-4 w-4" />
                </Button>
              </div>

              <div
                className={cn(
                  'text-sm tabular-nums',
                  expired ? 'text-destructive' : 'text-muted-foreground'
                )}
              >
                {expired
                  ? t('devices.addDevice.expired')
                  : t('devices.addDevice.expiresIn', { remaining: formatRemaining(remaining) })}
              </div>

              <p className="max-w-xs text-center text-xs text-muted-foreground">
                {t('devices.addDevice.passphraseHint')}
              </p>
            </>
          ) : null}
        </div>

        <div className="flex justify-end gap-2 border-t pt-4">
          {expired ? (
            <>
              <Button variant="outline" onClick={() => onOpenChange(false)} disabled={loading}>
                {t('devices.addDevice.actions.close')}
              </Button>
              <Button onClick={handleRegenerate} disabled={loading}>
                {loading ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <RefreshCw className="mr-2 h-4 w-4" />
                )}
                {t('devices.addDevice.actions.regenerate')}
              </Button>
            </>
          ) : (
            <Button variant="outline" onClick={handleCancel} disabled={loading}>
              {loading ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <XCircle className="mr-2 h-4 w-4" />
              )}
              {t('devices.addDevice.actions.cancel')}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

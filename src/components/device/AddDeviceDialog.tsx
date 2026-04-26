import { Check, Clock, Copy, Info, Loader2, RefreshCw, XCircle } from 'lucide-react'
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
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Progress } from '@/components/ui/progress'
import { createLogger } from '@/lib/logger'
import { cn } from '@/lib/utils'

const log = createLogger('add-device-dialog')

// 默认邀请有效期 — 用于计算进度条百分比；后端如调整该值，倒计时仍按
// expiresAtMs 显示真实剩余时间，进度条最多偏差视觉感受。
const DEFAULT_TTL_MS = 5 * 60 * 1000

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
  const [issuedAtMs, setIssuedAtMs] = useState<number | null>(null)
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
          // 复用的邀请 — 没有真实"签发时间"，按 TTL 倒推一个估算值
          setIssuedAtMs(state.currentInvitation.expiresAtMs - DEFAULT_TTL_MS)
        } else {
          const issued = await issuePairingInvitation()
          if (cancelled) return
          setInvitation(issued)
          setIssuedAtMs(Date.now())
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
      setIssuedAtMs(null)
      setError(null)
      setCopied(false)
    }
  }, [open])

  const remaining = invitation ? Math.max(0, invitation.expiresAtMs - now) : 0
  const expired = invitation ? remaining <= 0 : false
  const totalMs = invitation && issuedAtMs ? invitation.expiresAtMs - issuedAtMs : DEFAULT_TTL_MS
  const progress = invitation ? Math.max(0, Math.min(100, (remaining / totalMs) * 100)) : 0
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
      setIssuedAtMs(Date.now())
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

        {loading && !invitation ? (
          <div className="flex items-center justify-center gap-3 py-12 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('devices.addDevice.loading')}
          </div>
        ) : error && !invitation ? (
          <div className="flex flex-col items-center gap-3 py-10">
            <p className="text-sm text-destructive">{error}</p>
            <Button variant="outline" size="sm" onClick={handleRegenerate} disabled={loading}>
              <RefreshCw className="mr-2 h-3.5 w-3.5" />
              {t('devices.addDevice.actions.regenerate')}
            </Button>
          </div>
        ) : invitation ? (
          <div className="space-y-4 py-2">
            {/* 邀请码主卡 — 视觉焦点 */}
            <div
              className={cn(
                'rounded-2xl border bg-gradient-to-br p-5 transition-colors',
                expired
                  ? 'border-destructive/30 from-destructive/5 to-transparent'
                  : 'border-primary/20 from-primary/[0.04] to-transparent'
              )}
            >
              {/* 邀请码 */}
              <div
                className={cn(
                  'select-all text-center font-mono font-semibold tabular-nums text-foreground',
                  'text-[28px] tracking-[0.18em] sm:text-[32px] sm:tracking-[0.2em]',
                  expired && 'text-muted-foreground/50 line-through decoration-1'
                )}
                aria-label={invitation.code}
              >
                {display}
              </div>

              {/* 倒计时进度条 + 文字 */}
              <div className="mt-5 space-y-2">
                <Progress
                  value={progress}
                  className={cn(
                    'h-1',
                    expired && '[&>[data-slot=progress-indicator]]:bg-destructive'
                  )}
                />
                <div
                  className={cn(
                    'flex items-center justify-center gap-1.5 text-xs tabular-nums',
                    expired ? 'text-destructive' : 'text-muted-foreground'
                  )}
                >
                  <Clock className="h-3 w-3" />
                  {expired
                    ? t('devices.addDevice.expired')
                    : t('devices.addDevice.expiresIn', { remaining: formatRemaining(remaining) })}
                </div>
              </div>
            </div>

            {/* 提示：还需空间口令 */}
            <div className="flex items-start gap-2.5 rounded-lg bg-muted/50 px-3.5 py-2.5 text-xs text-muted-foreground">
              <Info className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span className="leading-relaxed">{t('devices.addDevice.passphraseHint')}</span>
            </div>
          </div>
        ) : null}

        <DialogFooter>
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
            <>
              <Button variant="ghost" onClick={handleCancel} disabled={loading || !invitation}>
                {loading ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <XCircle className="mr-2 h-4 w-4" />
                )}
                {t('devices.addDevice.actions.cancel')}
              </Button>
              <Button
                variant={copied ? 'outline' : 'default'}
                onClick={handleCopy}
                disabled={!invitation || loading}
              >
                {copied ? (
                  <>
                    <Check className="mr-2 h-4 w-4" />
                    {t('devices.addDevice.actions.copied')}
                  </>
                ) : (
                  <>
                    <Copy className="mr-2 h-4 w-4" />
                    {t('devices.addDevice.actions.copy')}
                  </>
                )}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

import { Check, Copy, Download, Loader2, Trash2 } from 'lucide-react'
import React from 'react'
import { useTranslation } from 'react-i18next'
import { Kbd } from '@/components/ui/kbd'
import { cn } from '@/lib/utils'

interface ClipboardActionBarProps {
  hasActiveItem: boolean
  copySuccess: boolean
  activeItemType?: 'text' | 'image' | 'link' | 'code' | 'file' | 'unknown'
  isActiveItemDownloaded?: boolean
  isActiveItemTransferring?: boolean
  onCopy: () => void
  onDelete: () => void
  onSyncToClipboard?: () => void
}

const ClipboardActionBar: React.FC<ClipboardActionBarProps> = ({
  hasActiveItem,
  copySuccess,
  activeItemType,
  isActiveItemDownloaded,
  isActiveItemTransferring,
  onCopy,
  onDelete,
  onSyncToClipboard,
}) => {
  const { t } = useTranslation()

  // Show "Sync to Clipboard" instead of Copy for undownloaded file items
  const showSyncButton =
    activeItemType === 'file' && isActiveItemDownloaded === false && onSyncToClipboard

  return (
    <div className="flex items-center justify-end border-t border-border/40 bg-card px-4 py-2 shrink-0">
      <div className="flex items-center gap-3">
        {showSyncButton ? (
          <button
            className={cn(
              'flex items-center gap-1.5 text-sm transition-colors',
              hasActiveItem && !isActiveItemTransferring
                ? 'text-foreground hover:text-primary cursor-pointer'
                : 'text-muted-foreground/50 cursor-default'
            )}
            onClick={hasActiveItem && !isActiveItemTransferring ? onSyncToClipboard : undefined}
            disabled={!hasActiveItem || isActiveItemTransferring}
          >
            {isActiveItemTransferring ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Download className="h-4 w-4" />
            )}
            <span>
              {isActiveItemTransferring
                ? t('clipboard.actionBar.syncing')
                : t('clipboard.actionBar.syncToClipboard')}
            </span>
          </button>
        ) : (
          <button
            className={cn(
              'flex items-center gap-1.5 text-sm transition-colors',
              hasActiveItem
                ? 'text-foreground hover:text-primary cursor-pointer'
                : 'text-muted-foreground/50 cursor-default'
            )}
            onClick={hasActiveItem ? onCopy : undefined}
            disabled={!hasActiveItem}
          >
            {copySuccess ? (
              <Check className="h-4 w-4 text-green-500" />
            ) : (
              <Copy className="h-4 w-4" />
            )}
            <span>{t('clipboard.actionBar.copy')}</span>
            <Kbd>C</Kbd>
          </button>
        )}

        <div className="w-px h-4 bg-border" />

        <button
          className={cn(
            'flex items-center gap-1.5 text-sm transition-colors',
            hasActiveItem
              ? 'text-foreground hover:text-destructive cursor-pointer'
              : 'text-muted-foreground/50 cursor-default'
          )}
          onClick={hasActiveItem ? onDelete : undefined}
          disabled={!hasActiveItem}
        >
          <Trash2 className="h-4 w-4" />
          <span>{t('clipboard.actionBar.delete')}</span>
          <Kbd>D</Kbd>
        </button>
      </div>
    </div>
  )
}

export default ClipboardActionBar

import { motion, AnimatePresence } from 'framer-motion'
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
  isCopyBlocked?: boolean
  copyBlockedReason?: string
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
  isCopyBlocked,
  copyBlockedReason,
  onCopy,
  onDelete,
  onSyncToClipboard,
}) => {
  const { t } = useTranslation()

  // Show "Sync to Clipboard" instead of Copy for undownloaded file items
  const showSyncButton =
    activeItemType === 'file' && isActiveItemDownloaded === false && onSyncToClipboard

  return (
    <div className="flex items-center justify-end px-4 py-3 shrink-0 bg-transparent">
      <div className="flex items-center gap-1">
        {showSyncButton ? (
          <motion.button
            whileTap={{ scale: 0.97 }}
            className={cn(
              'flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-colors duration-200',
              hasActiveItem && !isActiveItemTransferring
                ? 'text-primary hover:bg-primary/5 cursor-pointer'
                : 'text-muted-foreground/30 cursor-default'
            )}
            onClick={hasActiveItem && !isActiveItemTransferring ? onSyncToClipboard : undefined}
            disabled={!hasActiveItem || isActiveItemTransferring}
          >
            {isActiveItemTransferring ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Download className="h-3.5 w-3.5" />
            )}
            <span className="font-medium">
              {isActiveItemTransferring
                ? t('clipboard.actionBar.syncing')
                : t('clipboard.actionBar.syncToClipboard')}
            </span>
          </motion.button>
        ) : (
          <motion.button
            whileTap={{ scale: 0.97 }}
            className={cn(
              'flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-all duration-200 relative group',
              hasActiveItem && !isCopyBlocked
                ? 'text-foreground hover:bg-muted cursor-pointer'
                : 'text-muted-foreground/30 cursor-default'
            )}
            onClick={hasActiveItem && !isCopyBlocked ? onCopy : undefined}
            disabled={!hasActiveItem || isCopyBlocked}
            aria-disabled={isCopyBlocked}
            title={copyBlockedReason || undefined}
          >
            <AnimatePresence mode="wait" initial={false}>
              {copySuccess ? (
                <motion.div
                  key="check"
                  initial={{ scale: 0.5, opacity: 0 }}
                  animate={{ scale: 1, opacity: 1 }}
                  exit={{ scale: 0.5, opacity: 0 }}
                  transition={{ duration: 0.1 }}
                >
                  <Check className="h-3.5 w-3.5 text-green-500" />
                </motion.div>
              ) : (
                <motion.div
                  key="copy"
                  initial={{ scale: 0.8, opacity: 0 }}
                  animate={{ scale: 1, opacity: 1 }}
                  exit={{ scale: 0.8, opacity: 0 }}
                  transition={{ duration: 0.1 }}
                >
                  <Copy className="h-3.5 w-3.5" />
                </motion.div>
              )}
            </AnimatePresence>
            <span
              className={cn(
                'font-medium transition-colors',
                copySuccess ? 'text-green-600 dark:text-green-400' : ''
              )}
            >
              {copyBlockedReason ||
                (copySuccess
                  ? t('clipboard.actionBar.copied', '已复制')
                  : t('clipboard.actionBar.copy'))}
            </span>
            {!isCopyBlocked && hasActiveItem && (
              <Kbd className="bg-transparent opacity-40 group-hover:opacity-100 transition-opacity border-none h-4 min-w-4 p-0 text-[10px]">
                C
              </Kbd>
            )}
          </motion.button>
        )}

        <motion.button
          whileTap={{ scale: 0.97 }}
          className={cn(
            'flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-all duration-200 group',
            hasActiveItem
              ? 'text-muted-foreground hover:text-destructive hover:bg-destructive/5 cursor-pointer'
              : 'text-muted-foreground/30 cursor-default'
          )}
          onClick={hasActiveItem ? onDelete : undefined}
          disabled={!hasActiveItem}
        >
          <Trash2 className="h-3.5 w-3.5" />
          <span className="font-medium">{t('clipboard.actionBar.delete')}</span>
          {hasActiveItem && (
            <Kbd className="bg-transparent opacity-40 group-hover:opacity-100 transition-opacity border-none h-4 min-w-4 p-0 text-[10px]">
              D
            </Kbd>
          )}
        </motion.button>
      </div>
    </div>
  )
}

export default ClipboardActionBar

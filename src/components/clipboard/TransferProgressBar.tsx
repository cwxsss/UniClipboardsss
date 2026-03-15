import { ArrowDownToLine, ArrowUpFromLine } from 'lucide-react'
import React from 'react'
import { useTranslation } from 'react-i18next'
import { Progress } from '@/components/ui/progress'
import type { TransferProgressInfo } from '@/store/slices/fileTransferSlice'
import { formatFileSize } from '@/utils'

interface TransferProgressBarProps {
  progress: TransferProgressInfo
  variant?: 'compact' | 'detailed'
}

const TransferProgressBar: React.FC<TransferProgressBarProps> = ({
  progress,
  variant = 'compact',
}) => {
  const { t } = useTranslation()

  const percent =
    progress.totalChunks > 0
      ? Math.round((progress.chunksCompleted / progress.totalChunks) * 100)
      : 0

  const DirectionIcon = progress.direction === 'Sending' ? ArrowUpFromLine : ArrowDownToLine
  const directionLabel =
    progress.direction === 'Sending'
      ? t('clipboard.transfer.sending')
      : t('clipboard.transfer.receiving')

  if (variant === 'compact') {
    return (
      <div className="flex items-center gap-1.5 w-full">
        <DirectionIcon className="h-3 w-3 shrink-0 text-primary" />
        <Progress value={percent} className="h-1.5 flex-1" />
        <span className="text-xs text-muted-foreground shrink-0">{percent}%</span>
      </div>
    )
  }

  // Detailed variant for preview panel
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <DirectionIcon className="h-4 w-4 text-primary" />
        <span className="text-sm font-medium">{directionLabel}</span>
        <span className="text-sm text-muted-foreground ml-auto">{percent}%</span>
      </div>
      <Progress value={percent} className="h-2" />
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>
          {t('clipboard.transfer.progress', {
            transferred: formatFileSize(progress.bytesTransferred),
            total: progress.totalBytes ? formatFileSize(progress.totalBytes) : '?',
            percent,
          })}
        </span>
        <span>
          {t('clipboard.transfer.chunks', {
            completed: progress.chunksCompleted,
            total: progress.totalChunks,
          })}
        </span>
      </div>
    </div>
  )
}

export default TransferProgressBar

import { Code, ExternalLink, File, FileText, Image as ImageIcon } from 'lucide-react'
import React from 'react'
import type { DisplayClipboardItem } from './ClipboardContent'
import TransferProgressBar from './TransferProgressBar'
import {
  ClipboardCodeItem,
  ClipboardFileItem,
  ClipboardImageItem,
  ClipboardLinkItem,
  ClipboardTextItem,
} from '@/api/clipboardItems'
import { cn } from '@/lib/utils'
import { useAppSelector } from '@/store/hooks'
import { selectTransferByEntryId } from '@/store/slices/fileTransferSlice'

interface ClipboardItemRowProps {
  item: DisplayClipboardItem
  isActive: boolean
  onClick: () => void
  itemRef?: React.Ref<HTMLDivElement>
}

const typeIcons: Record<DisplayClipboardItem['type'], React.ElementType> = {
  text: FileText,
  image: ImageIcon,
  link: ExternalLink,
  code: Code,
  file: File,
  unknown: FileText,
}

function getPreviewText(item: DisplayClipboardItem): string {
  if (!item.content) return ''
  switch (item.type) {
    case 'text':
      return (item.content as ClipboardTextItem).display_text.slice(0, 80)
    case 'image': {
      const img = item.content as ClipboardImageItem
      if (img.width > 0 && img.height > 0) {
        return `Image(${img.width}x${img.height})`
      }
      return 'Image'
    }
    case 'link':
      return (item.content as ClipboardLinkItem).url
    case 'code':
      return (item.content as ClipboardCodeItem).code.split('\n')[0] ?? ''
    case 'file': {
      const names = (item.content as ClipboardFileItem).file_names
      return names.length > 0 ? names.join(', ') : 'File'
    }
    default:
      return ''
  }
}

const ClipboardItemRow: React.FC<ClipboardItemRowProps> = ({
  item,
  isActive,
  onClick,
  itemRef,
}) => {
  const Icon = typeIcons[item.type] ?? FileText
  const transfer = useAppSelector(state => selectTransferByEntryId(state, item.id))
  const isTransferring = transfer?.status === 'active'

  return (
    <div
      ref={itemRef}
      className={cn(
        'flex flex-col gap-1 py-2.5 px-3 rounded-lg cursor-pointer select-none transition-colors shrink-0 overflow-hidden',
        isActive ? 'bg-primary/10 text-foreground' : 'hover:bg-muted/50 text-foreground/80',
        isTransferring && 'ring-1 ring-primary/20'
      )}
      onClick={onClick}
    >
      <div className="flex items-center gap-3">
        <Icon
          className={cn('h-4 w-4 shrink-0', isActive ? 'text-primary' : 'text-muted-foreground')}
        />
        <span className="w-0 flex-grow truncate text-sm">{getPreviewText(item)}</span>
        <span className="text-xs text-muted-foreground shrink-0">{item.time}</span>
      </div>
      {isTransferring && transfer && (
        <div className="pl-7">
          <TransferProgressBar progress={transfer} variant="compact" />
        </div>
      )}
    </div>
  )
}

export default ClipboardItemRow

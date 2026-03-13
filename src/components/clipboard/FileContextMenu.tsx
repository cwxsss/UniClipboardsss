import { Copy, Download, FolderOpen, Loader2, Trash2 } from 'lucide-react'
import React from 'react'
import { useTranslation } from 'react-i18next'
import type { DisplayClipboardItem } from './ClipboardContent'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'

interface FileContextMenuProps {
  children: React.ReactNode
  itemId: string
  itemType: DisplayClipboardItem['type']
  isDownloaded: boolean
  isTransferring: boolean
  onCopy: (itemId: string) => void
  onDelete: (itemId: string) => void
  onSyncToClipboard: (itemId: string) => void
  onOpenFileLocation: (itemId: string) => void
}

const FileContextMenu: React.FC<FileContextMenuProps> = ({
  children,
  itemId,
  itemType,
  isDownloaded,
  isTransferring,
  onCopy,
  onDelete,
  onSyncToClipboard,
  onOpenFileLocation,
}) => {
  const { t } = useTranslation()

  const isFile = itemType === 'file'
  const showSyncAction = isFile && !isDownloaded
  const showCopyAction = !isFile || isDownloaded

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        {/* Sync to Clipboard (file not yet downloaded) */}
        {showSyncAction && (
          <ContextMenuItem disabled={isTransferring} onClick={() => onSyncToClipboard(itemId)}>
            {isTransferring ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <Download className="mr-2 h-4 w-4" />
            )}
            {isTransferring
              ? t('clipboard.contextMenu.syncing')
              : t('clipboard.contextMenu.syncToClipboard')}
          </ContextMenuItem>
        )}

        {/* Copy (for non-file types, or downloaded file types) */}
        {showCopyAction && (
          <ContextMenuItem onClick={() => onCopy(itemId)}>
            <Copy className="mr-2 h-4 w-4" />
            {t('clipboard.contextMenu.copy')}
            <ContextMenuShortcut>C</ContextMenuShortcut>
          </ContextMenuItem>
        )}

        <ContextMenuSeparator />

        {/* Open File Location (file type, downloaded) */}
        {isFile && isDownloaded && (
          <>
            <ContextMenuItem onClick={() => onOpenFileLocation(itemId)}>
              <FolderOpen className="mr-2 h-4 w-4" />
              {t('clipboard.contextMenu.openFileLocation')}
            </ContextMenuItem>
            <ContextMenuSeparator />
          </>
        )}

        {/* Delete */}
        <ContextMenuItem
          className="text-destructive focus:text-destructive"
          onClick={() => onDelete(itemId)}
        >
          <Trash2 className="mr-2 h-4 w-4" />
          {t('clipboard.contextMenu.delete')}
          <ContextMenuShortcut>D</ContextMenuShortcut>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

export default FileContextMenu

import { createContext } from 'react'
import type { UpdateMetadata } from '@/api/updater'

export interface UpdateContextType {
  updateInfo: UpdateMetadata | null
  isCheckingUpdate: boolean
  checkForUpdates: () => Promise<UpdateMetadata | null>
  installUpdate: () => Promise<void>
}

export const UpdateContext = createContext<UpdateContextType | undefined>(undefined)

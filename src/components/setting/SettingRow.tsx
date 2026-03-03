import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'

interface SettingRowProps {
  label?: string
  description?: string
  children?: ReactNode
  className?: string
}

export function SettingRow({ label, description, children, className }: SettingRowProps) {
  return (
    <div className={cn('flex items-center justify-between gap-4 px-4 py-3', className)}>
      {(label || description) && (
        <div className="space-y-0.5 min-w-0 flex-1">
          {label && <h4 className="text-sm font-medium">{label}</h4>}
          {description && (
            <p className="text-xs text-muted-foreground leading-snug">{description}</p>
          )}
        </div>
      )}
      {children && <div className="shrink-0">{children}</div>}
    </div>
  )
}

import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'

interface SettingGroupProps {
  title?: string
  children: ReactNode
  className?: string
}

export function SettingGroup({ title, children, className }: SettingGroupProps) {
  return (
    <div className={cn('space-y-1.5', className)}>
      {title && (
        <h3 className="text-xs font-medium text-muted-foreground px-1 uppercase tracking-wider">
          {title}
        </h3>
      )}
      <div className="rounded-lg border border-border/60 bg-card divide-y divide-border/40 overflow-hidden">
        {children}
      </div>
    </div>
  )
}

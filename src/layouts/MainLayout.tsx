import React, { ReactNode, useMemo, useState } from 'react'
import InsetSurface from '@/components/layout/InsetSurface'
import SidebarFooter from '@/components/layout/SidebarFooter'
import SidebarNavigation from '@/components/layout/SidebarNavigation'
import { ContentToolbar } from '@/components/TitleBar'
import { SidebarSlotContext } from '@/contexts/sidebar-slot-context'
import { usePlatform } from '@/hooks/usePlatform'
import { useWindowDrag } from '@/hooks/useWindowDrag'
import { useWindowFrame } from '@/hooks/useWindowFrame'

interface MainLayoutProps {
  children: ReactNode
  sidebarTitle?: ReactNode
}

interface SidebarAreaProps {
  title?: ReactNode
}

interface ContentToolbarProps {
  toolbarHostRef: (element: HTMLDivElement | null) => void
}

const SidebarArea: React.FC<SidebarAreaProps> = ({ title }) => {
  const dragHandlers = useWindowDrag()

  return (
    <aside data-tauri-drag-region {...dragHandlers} className="flex h-full w-12 shrink-0 flex-col">
      {title}
      <SidebarNavigation />
      <SidebarFooter />
    </aside>
  )
}

/**
 * Linux 系统标题栏布局。
 *
 * When the Linux system frame is enabled, the content uses a flat layout
 * instead of duplicating native window chrome with an inset panel.
 */
const LinuxMainLayout: React.FC<MainLayoutProps & SidebarAreaProps & ContentToolbarProps> = ({
  children,
  toolbarHostRef,
}) => {
  return (
    <>
      <SidebarArea />

      <main className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-card text-card-foreground">
        <ContentToolbar rightSlot={<div ref={toolbarHostRef} className="flex items-center" />} />
        <div className="min-h-0 flex-1">{children}</div>
      </main>
    </>
  )
}

/**
 * 自定义标题栏布局。
 *
 * The app-rendered frame uses one continuous shell background around the
 * sidebar and inset content panel on every desktop platform.
 */
const InsetMainLayout: React.FC<MainLayoutProps & SidebarAreaProps & ContentToolbarProps> = ({
  children,
  sidebarTitle,
  toolbarHostRef,
}) => {
  return (
    <>
      <SidebarArea title={sidebarTitle} />

      <main className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <ContentToolbar rightSlot={<div ref={toolbarHostRef} className="flex items-center" />} />
        <div className="flex min-h-0 flex-1 pb-2 pr-2">
          <InsetSurface className="h-full w-full flex-1 rounded-xl">{children}</InsetSurface>
        </div>
      </main>
    </>
  )
}

const MainLayout: React.FC<MainLayoutProps> = ({ children, sidebarTitle }) => {
  const { isLinux, isTauri } = usePlatform()
  const { useSystemWindowFrame } = useWindowFrame()
  const [contentToolbarHost, setContentToolbarHost] = useState<HTMLDivElement | null>(null)
  const sidebarSlot = useMemo(() => ({ contentToolbarHost }), [contentToolbarHost])

  if (isLinux && isTauri && useSystemWindowFrame) {
    return (
      <SidebarSlotContext value={sidebarSlot}>
        <LinuxMainLayout toolbarHostRef={setContentToolbarHost}>{children}</LinuxMainLayout>
      </SidebarSlotContext>
    )
  }

  return (
    <SidebarSlotContext value={sidebarSlot}>
      <InsetMainLayout sidebarTitle={sidebarTitle} toolbarHostRef={setContentToolbarHost}>
        {children}
      </InsetMainLayout>
    </SidebarSlotContext>
  )
}

export default MainLayout

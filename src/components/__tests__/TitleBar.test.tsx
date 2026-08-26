import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TitleBar } from '@/components/TitleBar'

const windowMocks = vi.hoisted(() => ({
  close: vi.fn().mockResolvedValue(undefined),
  isMaximized: vi.fn().mockResolvedValue(false),
  maximize: vi.fn().mockResolvedValue(undefined),
  minimize: vi.fn().mockResolvedValue(undefined),
  onResized: vi.fn().mockResolvedValue(() => {}),
  unmaximize: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => windowMocks,
}))

vi.mock('@/hooks/usePlatform', () => ({
  usePlatform: () => ({
    isWindows: true,
    isMac: false,
    isLinux: false,
    isTauri: true,
    hasCustomWindowControls: true,
    reduceVisualEffects: true,
  }),
}))

vi.mock('@/lib/ipc', () => ({
  commands: {
    setTrafficLightPosition: vi.fn().mockResolvedValue(undefined),
  },
}))

vi.mock('@/components/DevProfileIndicator', () => ({
  DevProfileIndicator: () => null,
}))

describe('TitleBar', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    windowMocks.isMaximized.mockResolvedValue(false)
  })

  it('在 Windows 中显示并启用窗口控制按钮', async () => {
    render(<TitleBar />)

    const closeControlContainer = screen.getByRole('button', { name: '关闭' }).parentElement
    expect(closeControlContainer).toHaveClass('mr-4')
    expect(closeControlContainer).not.toHaveClass('mr-2')

    fireEvent.click(screen.getByRole('button', { name: '最小化' }))
    fireEvent.click(screen.getByRole('button', { name: '最大化' }))
    fireEvent.click(screen.getByRole('button', { name: '关闭' }))

    await waitFor(() => {
      expect(windowMocks.minimize).toHaveBeenCalledOnce()
      expect(windowMocks.maximize).toHaveBeenCalledOnce()
      expect(windowMocks.close).toHaveBeenCalledOnce()
    })
  })

  it('最小化和最大化按钮也有悬停反馈，关闭按钮保留红色悬停反馈', () => {
    render(<TitleBar />)

    expect(screen.getByRole('button', { name: '最小化' })).toHaveClass('hover:bg-muted/70')
    expect(screen.getByRole('button', { name: '最大化' })).toHaveClass('hover:bg-muted/70')
    expect(screen.getByRole('button', { name: '关闭' })).toHaveClass('hover:bg-red-500/90')
  })
})

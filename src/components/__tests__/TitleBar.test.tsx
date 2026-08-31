import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ContentToolbar, TitleBar } from '@/components/TitleBar'

const windowMocks = vi.hoisted(() => ({
  close: vi.fn().mockResolvedValue(undefined),
  isMaximized: vi.fn().mockResolvedValue(false),
  maximize: vi.fn().mockResolvedValue(undefined),
  minimize: vi.fn().mockResolvedValue(undefined),
  onResized: vi.fn().mockResolvedValue(() => {}),
  startDragging: vi.fn().mockResolvedValue(undefined),
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

    expect(screen.getByRole('button', { name: '最小化' })).toHaveClass('hover:bg-foreground/10')
    expect(screen.getByRole('button', { name: '最大化' })).toHaveClass('hover:bg-foreground/10')
    expect(screen.getByRole('button', { name: '关闭' })).toHaveClass('hover:bg-red-500/90')
  })

  it('在标题栏空白区域开始窗口拖动，但不会从窗口控制按钮开始拖动', async () => {
    const { container } = render(<TitleBar />)
    const titleBar = container.firstElementChild as HTMLElement

    fireEvent.pointerDown(titleBar, { button: 0, clientX: 100, clientY: 10 })
    fireEvent.pointerMove(titleBar, { buttons: 1, clientX: 110, clientY: 10 })

    await waitFor(() => expect(windowMocks.startDragging).toHaveBeenCalledOnce())

    windowMocks.startDragging.mockClear()
    const minimizeButton = screen.getByRole('button', { name: '最小化' })
    fireEvent.pointerDown(minimizeButton, { button: 0, clientX: 100, clientY: 10 })
    fireEvent.pointerMove(minimizeButton, { buttons: 1, clientX: 110, clientY: 10 })

    expect(windowMocks.startDragging).not.toHaveBeenCalled()
  })

  it('ContentToolbar 在历史页标题栏空白区域开始窗口拖动', async () => {
    const { container } = render(<ContentToolbar />)
    const toolbar = container.firstElementChild as HTMLElement

    fireEvent.pointerDown(toolbar, { button: 0, clientX: 100, clientY: 10 })
    fireEvent.pointerMove(toolbar, { buttons: 1, clientX: 110, clientY: 10 })

    await waitFor(() => expect(windowMocks.startDragging).toHaveBeenCalledOnce())
  })

  it('ContentToolbar keeps the slot background draggable but excludes slot controls', async () => {
    const { getByRole, getByTestId } = render(
      <ContentToolbar
        rightSlot={
          <div data-testid="toolbar-slot">
            <button type="button" data-tauri-drag-region="false" aria-label="slot action">
              action
            </button>
          </div>
        }
      />
    )
    const slot = getByTestId('toolbar-slot')

    fireEvent.pointerDown(slot, { button: 0, clientX: 100, clientY: 10 })
    fireEvent.pointerMove(slot, { buttons: 1, clientX: 110, clientY: 10 })

    await waitFor(() => expect(windowMocks.startDragging).toHaveBeenCalledOnce())

    windowMocks.startDragging.mockClear()
    const action = getByRole('button', { name: 'slot action' })
    fireEvent.pointerDown(action, { button: 0, clientX: 100, clientY: 10 })
    fireEvent.pointerMove(action, { buttons: 1, clientX: 110, clientY: 10 })

    expect(windowMocks.startDragging).not.toHaveBeenCalled()
  })
})

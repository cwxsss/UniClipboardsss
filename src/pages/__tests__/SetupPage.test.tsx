import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import SetupPage from '@/pages/SetupPage'

const mocks = vi.hoisted(() => ({
  close: vi.fn().mockResolvedValue(undefined),
  isMaximized: vi.fn().mockResolvedValue(false),
  onResized: vi.fn().mockResolvedValue(() => {}),
  navigate: vi.fn(),
  finishPairing: vi.fn(),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: mocks.close,
    isMaximized: mocks.isMaximized,
    onResized: mocks.onResized,
  }),
}))

vi.mock('@/hooks/usePlatform', () => ({
  usePlatform: () => ({
    isWindows: true,
    isMac: false,
    isLinux: false,
    isTauri: true,
  }),
}))

vi.mock('@/hooks/useWindowFrame', () => ({
  useWindowFrame: () => ({ hasCustomWindowControls: true }),
}))

vi.mock('react-router', async importOriginal => {
  const actual = await importOriginal<typeof import('react-router')>()
  return { ...actual, useNavigate: () => mocks.navigate }
})

vi.mock('@/hooks/useSetupFlow', () => ({
  useSetupFlow: () => ({
    screen: { kind: 'entry' },
    loading: false,
    goEntry: vi.fn(),
    startCreateSpace: vi.fn(),
    startJoinSpace: vi.fn(),
    startImportConfig: vi.fn(),
    initializeSpace: vi.fn(),
    issueInvitation: vi.fn(),
    cancelInvitation: vi.fn(),
    redeemInvitation: vi.fn(),
    cancelJoin: vi.fn(),
    finishPairing: mocks.finishPairing,
  }),
}))

vi.mock('@/pages/setup/screens', () => ({
  EntryScreen: () => <div>Setup entry</div>,
  ImportConfigScreen: () => null,
  InitializeSpaceScreen: () => null,
  JoinPendingScreen: () => null,
  JoinRejectedScreen: () => null,
  PairingCompleteScreen: () => null,
  RedeemInvitationScreen: () => null,
  SetupBrandPanel: () => null,
  ShowInvitationScreen: () => null,
  SpaceReadyScreen: () => null,
}))

describe('SetupPage window controls', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.isMaximized.mockResolvedValue(false)
    mocks.onResized.mockResolvedValue(() => {})
  })

  it('keeps the close control available throughout onboarding', async () => {
    render(<SetupPage />)

    const closeButton = screen.getByRole('button', { name: '关闭' })
    expect(closeButton).toBeVisible()

    fireEvent.click(closeButton)

    await waitFor(() => expect(mocks.close).toHaveBeenCalledOnce())
  })
})

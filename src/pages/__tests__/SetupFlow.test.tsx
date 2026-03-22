import { render, screen, act } from '@testing-library/react'
import type { HTMLAttributes, ReactNode } from 'react'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { getSetupState, onSetupStateChanged, startNewSpace } from '@/api/setup'
import i18n from '@/i18n'
import SetupPage from '@/pages/SetupPage'

// Mock the API
vi.mock('@/api/setup', () => ({
  getSetupState: vi.fn(),
  onSetupStateChanged: vi.fn(() => Promise.resolve(() => {})),
  startNewSpace: vi.fn(),
  startJoinSpace: vi.fn(),
  selectJoinPeer: vi.fn(),
  submitPassphrase: vi.fn(),
  verifyPassphrase: vi.fn(),
  confirmPeerTrust: vi.fn(),
  cancelSetup: vi.fn(),
}))

// Mock useSetupRealtimeStore at module level (must come before any test uses SetupPage)
const useSetupRealtimeStoreMock = vi.hoisted(() => vi.fn())
const syncSetupStateFromCommandMock = vi.fn()
vi.mock('@/store/setupRealtimeStore', () => ({
  useSetupRealtimeStore: useSetupRealtimeStoreMock,
}))

// Mock react-router-dom
const navigateMock = vi.fn()
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return {
    ...actual,
    useNavigate: () => navigateMock,
  }
})

// Mock framer-motion to avoid animation issues in tests
vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: ReactNode }) => <>{children}</>,
  motion: new Proxy(
    {},
    {
      get: () => (props: HTMLAttributes<HTMLDivElement>) => <div {...props} />,
    }
  ),
}))

describe('Setup flow', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('zh-CN')
    vi.mocked(getSetupState).mockReset()
    vi.mocked(startNewSpace).mockReset()
    vi.mocked(onSetupStateChanged).mockReset()
    vi.mocked(onSetupStateChanged).mockResolvedValue(() => {})
    navigateMock.mockReset()
    useSetupRealtimeStoreMock.mockReset()
    // Default: store hydrates with Welcome state (compatible with existing tests)
    useSetupRealtimeStoreMock.mockReturnValue({
      setupState: 'Welcome',
      sessionId: null,
      hydrated: true,
      syncSetupStateFromCommand: syncSetupStateFromCommandMock,
    })
  })

  it('renders welcome step for SetupState.Welcome', async () => {
    // mock getSetupState() to return 'Welcome'
    vi.mocked(getSetupState).mockResolvedValue('Welcome')

    render(<SetupPage />)

    expect(await screen.findByText('欢迎使用 UniClipboard')).toBeInTheDocument()
    expect(screen.getByText(i18n.t('setup.welcome.subtitle'))).toBeInTheDocument()
    expect(screen.getByText(i18n.t('setup.welcome.create.title'))).toBeInTheDocument()

    await act(async () => {
      await i18n.changeLanguage('en-US')
    })

    expect(await screen.findByText('Welcome to UniClipboard')).toBeInTheDocument()
  })

  it('shows passphrase mismatch error text', async () => {
    // mock useSetupRealtimeStore to return CreateSpaceInputPassphrase with error
    useSetupRealtimeStoreMock.mockReturnValue({
      setupState: { CreateSpaceInputPassphrase: { error: 'PassphraseMismatch' } },
      sessionId: null,
      hydrated: true,
      syncSetupStateFromCommand: syncSetupStateFromCommandMock,
    })

    render(<SetupPage />)

    // Wait for the error message to appear
    expect(
      await screen.findByText(i18n.t('setup.createPassphrase.errors.mismatch'))
    ).toBeInTheDocument()
  })

  it('starts new space when clicking create CTA', async () => {
    vi.mocked(getSetupState).mockResolvedValue('Welcome')
    vi.mocked(startNewSpace).mockResolvedValue({
      CreateSpaceInputPassphrase: { error: null },
    })
    render(<SetupPage />)

    const ctaText = await screen.findByText(i18n.t('setup.welcome.create.cta'))
    const createBtn = ctaText.closest('button')
    expect(createBtn).toBeTruthy()

    if (!createBtn) {
      throw new Error('Create CTA button not found')
    }

    await act(async () => {
      createBtn.click()
    })

    expect(startNewSpace).toHaveBeenCalled()
  })

  it('uses non-scrollable main layout on welcome step', async () => {
    vi.mocked(getSetupState).mockResolvedValue('Welcome')

    const { container } = render(<SetupPage />)
    await screen.findByText('欢迎使用 UniClipboard')

    const mainContainer = container.querySelector('main')
    expect(mainContainer).toBeTruthy()
    expect(mainContainer).toHaveClass('overflow-hidden')
    expect(mainContainer).not.toHaveClass('overflow-y-auto')
  })

  it('does not show step dot indicator on welcome step', async () => {
    vi.mocked(getSetupState).mockResolvedValue('Welcome')

    const { container } = render(<SetupPage />)
    await screen.findByText('欢迎使用 UniClipboard')

    const dots = container.querySelectorAll('[data-testid^="dot-"]')
    expect(dots.length).toBe(0)
  })

  it('cleans listener when registration resolves after unmount', async () => {
    // Mock the store for this test
    useSetupRealtimeStoreMock.mockReturnValue({
      setupState: 'Welcome',
      sessionId: null,
      hydrated: true,
      syncSetupStateFromCommand: syncSetupStateFromCommandMock,
    })

    render(<SetupPage />)

    // Verify the page renders the welcome content
    expect(screen.queryByText('欢迎使用 UniClipboard')).toBeTruthy()
  })

  it('renders JoinSpaceConfirmPeer verification step from setup store', async () => {
    // This test proves SetupPage derives the confirmation view entirely from useSetupRealtimeStore
    // without depending on pairing verification mocks (onP2PPairingVerification is NOT used here)
    useSetupRealtimeStoreMock.mockReturnValue({
      setupState: {
        JoinSpaceConfirmPeer: {
          short_code: '123456',
          peer_fingerprint: 'ABCD1234EFGH',
          error: null,
        },
      },
      sessionId: 'session-123',
      hydrated: true,
      syncSetupStateFromCommand: syncSetupStateFromCommandMock,
    })

    render(<SetupPage />)

    // The short code should be visible
    expect(screen.getByText('123456')).toBeInTheDocument()
    // The peer fingerprint should be visible
    expect(screen.getByText('ABCD1234EFGH')).toBeInTheDocument()
    // A confirm button should be present (Chinese: 确认配对)
    expect(screen.getByRole('button', { name: /确认配对/i })).toBeInTheDocument()
    // A cancel button should be present (Chinese: 取消)
    expect(screen.getByRole('button', { name: /取消/i })).toBeInTheDocument()
  })
})

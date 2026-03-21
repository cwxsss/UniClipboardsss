import { render, screen, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { verifyP2PPairingPin } from '@/api/p2p'
import PairingDialog from '@/components/PairingDialog'
import { PairingNotificationProvider } from '@/components/PairingNotificationProvider'

const getP2PPeersMock = vi.hoisted(() => vi.fn())
const initiateP2PPairingMock = vi.hoisted(() => vi.fn())
const verifyP2PPairingPinMock = vi.hoisted(() => vi.fn())
const onP2PPairingVerificationMock = vi.hoisted(() => vi.fn())
const acceptP2PPairingMock = vi.hoisted(() => vi.fn())
const rejectP2PPairingMock = vi.hoisted(() => vi.fn())
const onSpaceAccessCompletedMock = vi.hoisted(() => vi.fn())

let verificationHandler:
  | ((event: { kind: string; sessionId: string; code?: string }) => void)
  | null = null

const classifyPairingErrorMock = vi.hoisted(() => (error?: string | null) => {
  const normalized = error?.toLowerCase() ?? ''
  if (normalized.includes('active pairing session exists')) {
    return 'active_session_exists'
  }
  if (
    normalized.includes('pairing session not found') ||
    normalized.includes('session_not_found')
  ) {
    return 'session_not_found'
  }
  if (normalized.includes('connection refused') || normalized.includes('daemon connection info')) {
    return 'daemon_unavailable'
  }
  return 'unknown'
})

vi.mock('@/api/p2p', () => ({
  getP2PPeers: getP2PPeersMock,
  initiateP2PPairing: initiateP2PPairingMock,
  verifyP2PPairingPin: verifyP2PPairingPinMock,
  onP2PPairingVerification: onP2PPairingVerificationMock,
  acceptP2PPairing: acceptP2PPairingMock,
  rejectP2PPairing: rejectP2PPairingMock,
  onSpaceAccessCompleted: onSpaceAccessCompletedMock,
  classifyPairingError: classifyPairingErrorMock,
}))

// Mock sonner toast so we can capture action handlers without needing a Toaster.
// The mock must expose .error() and .success() sub-methods that PairingNotificationProvider
// calls internally, in addition to the base toast() call used for request notifications.
const toastMock = vi.hoisted(() => {
  const fn = vi.fn() as ReturnType<typeof vi.fn> & {
    error: ReturnType<typeof vi.fn>
    success: ReturnType<typeof vi.fn>
  }
  fn.error = vi.fn()
  fn.success = vi.fn()
  return fn
})
vi.mock('sonner', () => ({
  toast: toastMock,
}))

describe('PairingDialog', () => {
  beforeEach(() => {
    verificationHandler = null
    getP2PPeersMock.mockResolvedValue([])
    initiateP2PPairingMock.mockResolvedValue({ success: true, sessionId: 'session-1' })
    verifyP2PPairingPinMock.mockResolvedValue(undefined)
    onP2PPairingVerificationMock.mockImplementation(async callback => {
      verificationHandler = callback
      return vi.fn()
    })
  })

  it('shows loading state after confirming PIN match', async () => {
    const user = userEvent.setup()

    render(<PairingDialog open onClose={vi.fn()} />)

    await act(async () => {})

    expect(verificationHandler).not.toBeNull()

    act(() => {
      verificationHandler?.({
        kind: 'verification',
        sessionId: 'session-1',
        code: '123456',
      })
    })

    const confirmButton = await screen.findByRole('button', {
      name: /确认匹配|Confirm Match/i,
    })

    await user.click(confirmButton)

    expect(verifyP2PPairingPin).toHaveBeenCalledWith({
      sessionId: 'session-1',
      pinMatches: true,
    })
    expect(confirmButton).toBeDisabled()
    expect(confirmButton).toHaveTextContent(/正在验证|Verifying/i)
  })

  it('keeps initiator flow on the active session until completion', async () => {
    const user = userEvent.setup()

    render(<PairingDialog open onClose={vi.fn()} />)

    await act(async () => {})

    act(() => {
      verificationHandler?.({
        kind: 'verification',
        sessionId: 'session-1',
        code: '123456',
      })
    })

    const confirmButton = await screen.findByRole('button', {
      name: /确认匹配|Confirm Match/i,
    })
    await user.click(confirmButton)

    act(() => {
      verificationHandler?.({
        kind: 'verifying',
        sessionId: 'other-session',
      })
    })

    expect(confirmButton).toHaveTextContent(/正在验证|Verifying/i)

    act(() => {
      verificationHandler?.({
        kind: 'complete',
        sessionId: 'other-session',
      })
    })

    expect(screen.queryByText(/配对成功|Pairing Successful/i)).not.toBeInTheDocument()

    act(() => {
      verificationHandler?.({
        kind: 'complete',
        sessionId: 'session-1',
      })
    })

    expect(await screen.findAllByText(/配对成功|Pairing Successful/i)).toHaveLength(2)
  })

  it('shows localized failure only for the active initiator session', async () => {
    render(<PairingDialog open onClose={vi.fn()} />)

    await act(async () => {})

    act(() => {
      verificationHandler?.({
        kind: 'verification',
        sessionId: 'session-1',
        code: '123456',
      })
    })

    expect(await screen.findByText('123456')).toBeInTheDocument()

    act(() => {
      verificationHandler?.({
        kind: 'failed',
        sessionId: 'other-session',
        error: 'pairing session not found',
      })
    })

    expect(screen.queryByText(/配对失败|Pairing Failed/i)).not.toBeInTheDocument()

    act(() => {
      verificationHandler?.({
        kind: 'failed',
        sessionId: 'session-1',
        error: 'pairing session not found',
      })
    })

    expect(await screen.findAllByText(/配对失败|Pairing Failed/i)).toHaveLength(2)
    expect(
      await screen.findAllByText(
        /配对会话已过期或已关闭|The pairing session expired or was already closed/i
      )
    ).toHaveLength(2)
  })
})

describe('PairingDialog failure states', () => {
  beforeEach(() => {
    verificationHandler = null
    getP2PPeersMock.mockResolvedValue([
      {
        peerId: 'peer-1',
        deviceName: 'Desk',
        addresses: [],
        isPaired: false,
        connected: true,
      },
    ])
    onP2PPairingVerificationMock.mockImplementation(async callback => {
      verificationHandler = callback
      return vi.fn()
    })
  })

  it('shows localized active session error for initiator failures', async () => {
    const user = userEvent.setup()
    initiateP2PPairingMock.mockResolvedValue({
      success: false,
      sessionId: '',
      error: 'active pairing session exists',
    })

    render(<PairingDialog open onClose={vi.fn()} />)

    await act(async () => {})
    await user.click(screen.getByText('Desk').closest('button')!)

    expect(
      await screen.findAllByText(
        /已有正在进行的配对，请稍后再试|Another pairing session is already in progress/i
      )
    ).toHaveLength(2)
  })

  it('shows localized session expired error for missing sessions', async () => {
    const user = userEvent.setup()
    initiateP2PPairingMock.mockResolvedValue({
      success: false,
      sessionId: '',
      error: 'pairing session not found',
    })

    render(<PairingDialog open onClose={vi.fn()} />)

    await act(async () => {})
    await user.click(screen.getByText('Desk').closest('button')!)

    expect(
      await screen.findAllByText(
        /配对会话已过期或已关闭|The pairing session expired or was already closed/i
      )
    ).toHaveLength(2)
  })

  it('shows localized daemon unavailable error when initiate throws', async () => {
    const user = userEvent.setup()
    initiateP2PPairingMock.mockRejectedValue(
      new Error('failed to call daemon pairing route /pairing/initiate: connection refused')
    )

    render(<PairingDialog open onClose={vi.fn()} />)

    await act(async () => {})
    await user.click(screen.getByText('Desk').closest('button')!)

    expect(
      await screen.findAllByText(
        /配对 daemon 不可用，请启动桌面服务后重试|The pairing daemon is unavailable. Start the desktop service and try again/i
      )
    ).toHaveLength(2)
  })
})

describe('PairingNotificationProvider — accept->verification race regression', () => {
  // Captures the handler registered via onP2PPairingVerification so tests
  // can push synthetic events.
  let capturedVerificationHandler:
    | ((event: {
        kind: string
        sessionId: string
        code?: string
        deviceName?: string
        peerId?: string
        error?: string
      }) => void)
    | null = null

  beforeEach(() => {
    vi.clearAllMocks()
    capturedVerificationHandler = null

    // onP2PPairingVerification: capture handler, return a no-op unlisten
    onP2PPairingVerificationMock.mockImplementation(
      async (callback: typeof capturedVerificationHandler) => {
        capturedVerificationHandler = callback
        return vi.fn()
      }
    )

    // onSpaceAccessCompleted: just return a no-op unlisten
    onSpaceAccessCompletedMock.mockImplementation(async () => vi.fn())

    // acceptP2PPairing resolves successfully
    acceptP2PPairingMock.mockResolvedValue(undefined)

    // rejectP2PPairing resolves successfully
    rejectP2PPairingMock.mockResolvedValue(undefined)
  })

  it('verification event immediately after accept is not dropped — PIN dialog appears', async () => {
    render(<PairingNotificationProvider />)

    // Let useEffects settle so the listener is registered.
    await act(async () => {})

    expect(capturedVerificationHandler).not.toBeNull()

    // Step 1: backend sends a pairing request.
    act(() => {
      capturedVerificationHandler!({
        kind: 'request',
        sessionId: 'session-abc',
        deviceName: 'PeerB',
        peerId: 'peer-id-b',
      })
    })

    // The toast() call should have been made with an action button.
    expect(toastMock).toHaveBeenCalled()
    const toastCall = toastMock.mock.calls[0]
    // toastMock is called as toast(title, { action: { onClick }, ... })
    const toastOptions = toastCall[1] as { action?: { onClick?: () => void } }
    expect(toastOptions.action?.onClick).toBeDefined()

    // Step 2: user clicks Accept.
    // The onClick is synchronous; it writes the ref and calls acceptP2PPairing.
    act(() => {
      toastOptions.action!.onClick!()
    })

    // Step 3: backend immediately pushes verification for the same session
    // (before the acceptP2PPairing promise resolves and before the next render).
    act(() => {
      capturedVerificationHandler!({
        kind: 'verification',
        sessionId: 'session-abc',
        code: '123456',
        deviceName: 'PeerB',
        peerId: 'peer-id-b',
      })
    })

    // Step 4: PIN dialog must now be visible — session guard must NOT have
    // discarded the verification event.
    await screen.findByText('123456')

    // The dialog should be open (pin code rendered).
    expect(screen.getByText('123456')).toBeInTheDocument()
  })

  it('accept failure rolls back session — subsequent verification is ignored', async () => {
    // acceptP2PPairing rejects to simulate a backend error.
    acceptP2PPairingMock.mockRejectedValue(new Error('accept failed'))

    render(<PairingNotificationProvider />)

    await act(async () => {})

    expect(capturedVerificationHandler).not.toBeNull()

    // Send request event.
    act(() => {
      capturedVerificationHandler!({
        kind: 'request',
        sessionId: 'session-fail',
        deviceName: 'PeerB',
        peerId: 'peer-id-b',
      })
    })

    const toastOptions = toastMock.mock.calls[0][1] as { action?: { onClick?: () => void } }

    // Click accept — acceptP2PPairing will reject.
    act(() => {
      toastOptions.action!.onClick!()
    })

    // Wait for the rejection to settle so the rollback runs.
    await act(async () => {})

    // After rollback, a verification for the failed session should be ignored.
    act(() => {
      capturedVerificationHandler!({
        kind: 'verification',
        sessionId: 'session-fail',
        code: '999999',
        deviceName: 'PeerB',
        peerId: 'peer-id-b',
      })
    })

    // PIN dialog must NOT be shown — no pin code on screen.
    expect(screen.queryByText('999999')).not.toBeInTheDocument()
  })
})

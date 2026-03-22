// @vitest-environment jsdom

import { act, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const acceptP2PPairingMock = vi.fn(() => Promise.resolve())
const rejectP2PPairingMock = vi.fn(() => Promise.resolve())
const toastMock = Object.assign(vi.fn(), { error: vi.fn() })
const onP2PPairingVerificationMock = vi.fn()
const onSpaceAccessCompletedMock = vi.fn()

const classifyPairingError = (error?: string | null) => {
  const normalized = error?.toLowerCase() ?? ''
  if (normalized.includes('active pairing session exists')) {
    return 'active_session_exists'
  }
  if (normalized.includes('pairing session not found')) {
    return 'session_not_found'
  }
  if (normalized.includes('connection refused') || normalized.includes('daemon connection info')) {
    return 'daemon_unavailable'
  }
  return 'unknown'
}

type PairingRealtimeEvent = {
  kind: string
  sessionId: string
  peerId?: string
  deviceName?: string
  code?: string
}

type SpaceAccessEvent = {
  sessionId: string
  success: boolean
  reason?: string
}

let capturedVerificationCallback: ((event: PairingRealtimeEvent) => void) | null = null
let capturedSpaceAccessCallback: ((event: SpaceAccessEvent) => void) | null = null

vi.mock('@/api/p2p', () => ({
  acceptP2PPairing: acceptP2PPairingMock,
  rejectP2PPairing: rejectP2PPairingMock,
  onP2PPairingVerification: onP2PPairingVerificationMock,
  onSpaceAccessCompleted: onSpaceAccessCompletedMock,
  classifyPairingError,
}))

const getSetupStateMock = vi.fn()
vi.mock('@/api/setup', () => ({
  getSetupState: getSetupStateMock,
}))

vi.mock('sonner', () => ({
  toast: toastMock,
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string; device?: string }) => {
      if (options?.device) return options.device
      if (options?.defaultValue) return options.defaultValue

      const messages: Record<string, string> = {
        'pairing.failed.title': 'Pairing failed',
        'pairing.failed.errors.activeSession': 'Another pairing session is already in progress',
        'pairing.failed.errors.sessionExpired': 'The pairing session expired or was already closed',
        'pairing.failed.errors.daemonUnavailable':
          'The pairing daemon is unavailable. Start the desktop service and try again',
      }

      return messages[key] ?? key
    },
  }),
}))

vi.mock('@/components/PairingPinDialog', () => ({
  default: (props: { open: boolean; pinCode: string; phase?: string; peerDeviceName?: string }) => (
    <div data-testid="pairing-pin-dialog">
      {JSON.stringify({
        open: props.open,
        pinCode: props.pinCode,
        phase: props.phase,
        peerDeviceName: props.peerDeviceName,
      })}
    </div>
  ),
}))

describe('PairingNotificationProvider realtime', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    acceptP2PPairingMock.mockImplementation(() => Promise.resolve())
    rejectP2PPairingMock.mockImplementation(() => Promise.resolve())
    capturedVerificationCallback = null
    capturedSpaceAccessCallback = null
    onP2PPairingVerificationMock.mockImplementation(
      (callback: (event: PairingRealtimeEvent) => void) => {
        capturedVerificationCallback = callback
        return Promise.resolve(() => {})
      }
    )
    onSpaceAccessCompletedMock.mockImplementation((callback: (event: SpaceAccessEvent) => void) => {
      capturedSpaceAccessCallback = callback
      return Promise.resolve(() => {})
    })
  })

  it('routes request and verification events through daemon realtime envelopes', async () => {
    const { PairingNotificationProvider } = await import('@/components/PairingNotificationProvider')
    render(<PairingNotificationProvider />)

    act(() => {
      capturedVerificationCallback?.({
        kind: 'request',
        sessionId: 'session-1',
        peerId: 'peer-1',
        deviceName: 'Desk',
      })
    })

    const toastOptions = toastMock.mock.calls[0]?.[1]
    expect(toastOptions?.action?.label).toBe('Accept')

    await act(async () => {
      await toastOptions?.action?.onClick()
    })

    expect(acceptP2PPairingMock).toHaveBeenCalledWith('session-1')

    await act(async () => {
      capturedVerificationCallback?.({
        kind: 'verification',
        sessionId: 'session-1',
        peerId: 'peer-1',
        deviceName: 'Desk',
        code: '123456',
      })
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"open":true')
    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"pinCode":"123456"')
  })

  it('keeps passive flow on verification -> verifying -> success for the active session only', async () => {
    vi.useFakeTimers()

    const { PairingNotificationProvider } = await import('@/components/PairingNotificationProvider')
    render(<PairingNotificationProvider />)

    act(() => {
      capturedVerificationCallback?.({
        kind: 'request',
        sessionId: 'session-1',
        peerId: 'peer-1',
        deviceName: 'Desk',
      })
    })

    const toastOptions = toastMock.mock.calls[0]?.[1]
    await act(async () => {
      await toastOptions?.action?.onClick()
    })

    await act(async () => {
      capturedVerificationCallback?.({
        kind: 'verification',
        sessionId: 'session-1',
        peerId: 'peer-1',
        deviceName: 'Desk',
        code: '123456',
      })
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"phase":"display"')

    act(() => {
      capturedVerificationCallback?.({
        kind: 'complete',
        sessionId: 'other-session',
      })
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"phase":"display"')

    await act(async () => {
      capturedVerificationCallback?.({
        kind: 'verifying',
        sessionId: 'session-1',
      })
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"phase":"verifying"')

    await act(async () => {
      capturedVerificationCallback?.({
        kind: 'complete',
        sessionId: 'session-1',
      })
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"phase":"verifying"')

    act(() => {
      capturedSpaceAccessCallback?.({
        sessionId: 'session-1',
        success: true,
      })
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"phase":"success"')

    await act(async () => {
      vi.advanceTimersByTime(2000)
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"open":false')
    vi.useRealTimers()
  })

  it('shows specific toast copy when accept pairing fails', async () => {
    acceptP2PPairingMock.mockRejectedValue(new Error('active pairing session exists'))

    const { PairingNotificationProvider } = await import('@/components/PairingNotificationProvider')
    render(<PairingNotificationProvider />)

    act(() => {
      capturedVerificationCallback?.({
        kind: 'request',
        sessionId: 'session-accept-error',
        peerId: 'peer-1',
        deviceName: 'Desk',
      })
    })

    const toastOptions = toastMock.mock.calls[0]?.[1]
    await act(async () => {
      await toastOptions?.action?.onClick()
    })

    expect(toastMock.error).toHaveBeenCalledWith('Pairing failed', {
      description: 'Another pairing session is already in progress',
    })
  })

  it('shows specific toast copy when reject pairing fails', async () => {
    rejectP2PPairingMock.mockRejectedValue(new Error('pairing session not found'))

    const { PairingNotificationProvider } = await import('@/components/PairingNotificationProvider')
    render(<PairingNotificationProvider />)

    act(() => {
      capturedVerificationCallback?.({
        kind: 'request',
        sessionId: 'session-reject-error',
        peerId: 'peer-1',
        deviceName: 'Desk',
      })
    })

    const toastOptions = toastMock.mock.calls[0]?.[1]
    await act(async () => {
      await toastOptions?.cancel?.onClick()
    })

    expect(toastMock.error).toHaveBeenCalledWith('Pairing failed', {
      description: 'The pairing session expired or was already closed',
    })
  })

  it('does not call getSetupState — PairingNotificationProvider has no setup awareness', async () => {
    const { PairingNotificationProvider } = await import('@/components/PairingNotificationProvider')
    render(<PairingNotificationProvider />)

    // getSetupStateMock should never have been called during render
    expect(getSetupStateMock).not.toHaveBeenCalled()
  })
})

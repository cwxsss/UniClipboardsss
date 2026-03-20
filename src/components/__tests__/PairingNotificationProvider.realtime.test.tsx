// @vitest-environment jsdom

import { act, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const acceptP2PPairingMock = vi.fn(() => Promise.resolve())
const rejectP2PPairingMock = vi.fn(() => Promise.resolve())
const toastMock = Object.assign(vi.fn(), { error: vi.fn() })
let capturedRealtimeCallback: ((event: any) => void) | null = null

vi.mock('@/api/p2p', () => ({
  acceptP2PPairing: acceptP2PPairingMock,
  rejectP2PPairing: rejectP2PPairingMock,
}))

vi.mock('@/api/realtime', () => ({
  onDaemonRealtimeEvent: vi.fn((callback: (event: any) => void) => {
    capturedRealtimeCallback = callback
    return Promise.resolve(() => {})
  }),
}))

vi.mock('sonner', () => ({
  toast: toastMock,
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (_key: string, options?: { defaultValue?: string; device?: string }) =>
      options?.device ?? options?.defaultValue ?? '',
  }),
}))

vi.mock('@/components/PairingPinDialog', () => ({
  default: (props: any) => (
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
    capturedRealtimeCallback = null
  })

  it('routes request and verification events through daemon realtime envelopes', async () => {
    const { PairingNotificationProvider } = await import('@/components/PairingNotificationProvider')
    render(<PairingNotificationProvider />)

    act(() => {
      capturedRealtimeCallback?.({
        topic: 'pairing',
        type: 'pairing.updated',
        ts: 1,
        payload: {
          sessionId: 'session-1',
          status: 'request',
          peerId: 'peer-1',
          deviceName: 'Desk',
        },
      })
    })

    const toastOptions = toastMock.mock.calls[0]?.[1]
    expect(toastOptions?.action?.label).toBe('Accept')

    await act(async () => {
      await toastOptions?.action?.onClick()
    })

    expect(acceptP2PPairingMock).toHaveBeenCalledWith('session-1')

    act(() => {
      capturedRealtimeCallback?.({
        topic: 'pairing',
        type: 'pairing.verificationRequired',
        ts: 2,
        payload: {
          sessionId: 'session-1',
          peerId: 'peer-1',
          deviceName: 'Desk',
          code: '123456',
        },
      })
    })

    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"open":true')
    expect(screen.getByTestId('pairing-pin-dialog').textContent).toContain('"pinCode":"123456"')
  })
})

// @vitest-environment jsdom

import { act, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const acceptP2PPairingMock = vi.fn(() => Promise.resolve())
const rejectP2PPairingMock = vi.fn(() => Promise.resolve())
const toastMock = Object.assign(vi.fn(), { error: vi.fn() })
const onP2PPairingVerificationMock = vi.fn()
const onSpaceAccessCompletedMock = vi.fn()
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
let _capturedSpaceAccessCallback: ((event: SpaceAccessEvent) => void) | null = null

vi.mock('@/api/p2p', () => ({
  acceptP2PPairing: acceptP2PPairingMock,
  rejectP2PPairing: rejectP2PPairingMock,
  onP2PPairingVerification: onP2PPairingVerificationMock,
  onSpaceAccessCompleted: onSpaceAccessCompletedMock,
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
    capturedVerificationCallback = null
    _capturedSpaceAccessCallback = null
    onP2PPairingVerificationMock.mockImplementation(
      (callback: (event: PairingRealtimeEvent) => void) => {
        capturedVerificationCallback = callback
        return Promise.resolve(() => {})
      }
    )
    onSpaceAccessCompletedMock.mockImplementation((callback: (event: SpaceAccessEvent) => void) => {
      _capturedSpaceAccessCallback = callback
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

    act(() => {
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
})

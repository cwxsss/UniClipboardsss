import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { daemonClient } from '@/api/daemon/client'
import type { DaemonConfig } from '@/api/daemon/types'

const TEST_CONFIG: DaemonConfig = {
  baseUrl: 'http://127.0.0.1:9999',
  wsUrl: 'ws://127.0.0.1:9999/ws',
  pid: 12345,
  token: 'test-bearer-token',
}

describe('DaemonClient empty success responses', () => {
  beforeEach(() => {
    daemonClient.destroy()
  })

  afterEach(() => {
    daemonClient.destroy()
    vi.unstubAllGlobals()
  })

  it('treats 200 OK with an empty body as success for void requests', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(
          new Response(
            JSON.stringify({
              sessionToken: 'jwt-empty-200',
              expiresInSecs: 300,
              refreshAtSecs: 240,
            }),
            {
              status: 200,
              headers: { 'Content-Type': 'application/json' },
            }
          )
        )
        .mockResolvedValueOnce(new Response(null, { status: 200 }))
    )

    daemonClient.initialize(TEST_CONFIG)

    await expect(
      daemonClient.request<void>('/pairing/accept', {
        method: 'POST',
        body: { sessionId: 'session-xyz' },
      })
    ).resolves.toBeUndefined()
  })

  it('still treats 204 No Content as success for void requests', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(
          new Response(
            JSON.stringify({
              sessionToken: 'jwt-empty-204',
              expiresInSecs: 300,
              refreshAtSecs: 240,
            }),
            {
              status: 200,
              headers: { 'Content-Type': 'application/json' },
            }
          )
        )
        .mockResolvedValueOnce(new Response(null, { status: 204 }))
    )

    daemonClient.initialize(TEST_CONFIG)

    await expect(
      daemonClient.request<void>('/pairing/reject', {
        method: 'POST',
        body: { sessionId: 'session-xyz' },
      })
    ).resolves.toBeUndefined()
  })
})

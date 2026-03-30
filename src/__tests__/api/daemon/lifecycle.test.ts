/**
 * Integration tests for the daemon lifecycle API module.
 *
 * Covers:
 * - POST /lifecycle/ready
 *
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { setupFetchMock, teardownFetchMock, mockResponse } from './_test-helpers'
import { signalLifecycleReady } from '@/api/daemon/lifecycle'

describe('Lifecycle API', () => {
  let mockFetch: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    const { mockFetch: mf } = setupFetchMock()
    mockFetch = mf
  })

  afterEach(() => {
    teardownFetchMock()
  })

  it('posts lifecycle ready to the daemon', async () => {
    mockFetch.mockResolvedValueOnce(mockResponse({}, 204))

    await signalLifecycleReady()

    expect(mockFetch).toHaveBeenCalledTimes(1)
    const [url, options] = mockFetch.mock.calls[0] as [URL | string, RequestInit]
    expect(String(url)).toContain('/lifecycle/ready')
    expect(options.method).toBe('POST')
  })
})

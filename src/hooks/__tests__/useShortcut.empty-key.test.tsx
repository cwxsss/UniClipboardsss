import { render } from '@testing-library/react'
import { useHotkeys } from 'react-hotkeys-hook'
import { describe, expect, it, vi } from 'vitest'
import { ShortcutProvider } from '@/contexts/ShortcutContext'
import { useShortcut } from '@/hooks/useShortcut'

vi.mock('react-hotkeys-hook', () => ({
  useHotkeys: vi.fn(),
}))

function EmptyShortcut() {
  useShortcut({
    key: '',
    scope: 'global',
    handler: vi.fn(),
  })
  return null
}

describe('useShortcut empty bindings', () => {
  it('does not activate either listener when the effective key is empty', () => {
    vi.mocked(useHotkeys).mockClear()

    render(
      <ShortcutProvider>
        <EmptyShortcut />
      </ShortcutProvider>
    )

    expect(vi.mocked(useHotkeys)).toHaveBeenCalledTimes(2)
    for (const call of vi.mocked(useHotkeys).mock.calls) {
      expect(call[2]).toEqual(expect.objectContaining({ enabled: false }))
    }
  })
})

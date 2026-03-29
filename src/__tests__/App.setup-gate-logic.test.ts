import { describe, expect, it } from 'vitest'
import { isSetupGateActive, shouldKeepSetupCompletionStep } from '@/App'

describe('App setup gate logic', () => {
  it('keeps setup active while the shared setup store is hydrating', () => {
    expect(isSetupGateActive(null, false, false)).toBe(true)
  })

  it('skips setup when hydration is complete and setup is already completed', () => {
    expect(isSetupGateActive('Completed', true, false)).toBe(false)
  })

  it('keeps the completed step visible after a live transition to completed', () => {
    expect(shouldKeepSetupCompletionStep('Welcome', 'Completed', true)).toBe(true)
    expect(isSetupGateActive('Completed', true, true)).toBe(true)
  })
})

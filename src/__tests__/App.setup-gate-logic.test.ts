import { describe, expect, it } from 'vitest'
import { isSetupGateActive, shouldKeepSetupCompletionStep } from '@/App'
import type { SetupFlow } from '@/store/setupRealtimeStore'

const completed: SetupFlow = { kind: 'completed', deviceName: 'host' }
const entry: SetupFlow = { kind: 'entry' }
const loading: SetupFlow = { kind: 'loading' }

describe('App setup gate logic', () => {
  it('keeps setup active while the shared setup store is hydrating', () => {
    expect(isSetupGateActive(loading, false, false)).toBe(true)
  })

  it('skips setup when hydration is complete and the flow is already completed', () => {
    expect(isSetupGateActive(completed, true, false)).toBe(false)
  })

  it('keeps the completed step visible after a live transition to completed', () => {
    expect(shouldKeepSetupCompletionStep(entry, completed, true)).toBe(true)
    expect(isSetupGateActive(completed, true, true)).toBe(true)
  })

  it('does not latch the completion step when the device was already completed at launch', () => {
    expect(shouldKeepSetupCompletionStep(loading, completed, true)).toBe(false)
    expect(shouldKeepSetupCompletionStep(completed, completed, true)).toBe(false)
  })
})

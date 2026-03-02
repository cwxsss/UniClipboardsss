import { describe, expect, it } from 'vitest'
import { bumpVersion, parseSemver } from '../bump-version-lib.js'

describe('bumpVersion prerelease patch', () => {
  it('keeps base version when creating first alpha prerelease', () => {
    expect(bumpVersion('0.1.0', 'patch', 'alpha')).toBe('0.1.0-alpha.1')
  })

  it('increments alpha prerelease number on repeat', () => {
    expect(bumpVersion('0.1.0-alpha.1', 'patch', 'alpha')).toBe('0.1.0-alpha.2')
  })
})

describe('parseSemver target version', () => {
  it('accepts explicit prerelease target version', () => {
    const parsed = parseSemver('0.1.0-alpha.2')
    expect(parsed.prerelease).toBe('alpha')
    expect(parsed.prereleaseVersion).toBe(2)
  })
})

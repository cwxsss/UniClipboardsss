import * as childProcess from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { bumpVersion, parseSemver } from '../bump-version-lib.js'
import { updateBunLock } from '../bump-version.js'

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

describe('updateBunLock', () => {
  const tempDirs: string[] = []
  const originalCwd = process.cwd()

  afterEach(() => {
    process.chdir(originalCwd)
    vi.restoreAllMocks()
    while (tempDirs.length > 0) {
      const dir = tempDirs.pop()
      if (dir) {
        fs.rmSync(dir, { recursive: true, force: true })
      }
    }
  })

  it('regenerates bun.lock via bun install --lockfile-only', () => {
    const repoDir = fs.mkdtempSync(path.join(os.tmpdir(), 'bump-version-'))
    tempDirs.push(repoDir)
    process.chdir(repoDir)
    fs.writeFileSync(path.join(repoDir, 'bun.lock'), '{}\n', 'utf8')

    const execSpy = vi.spyOn(childProcess, 'execFileSync').mockReturnValue(Buffer.from(''))

    const result = updateBunLock(false)

    expect(result.skipped).toBe(false)
    expect(result.path).toEndWith('bun.lock')
    expect(execSpy).toHaveBeenCalledTimes(1)
    expect(execSpy.mock.calls[0][0]).toBe('bun')
    expect(execSpy.mock.calls[0][1]).toEqual(['install', '--lockfile-only'])
    expect(execSpy.mock.calls[0][2]).toMatchObject({ stdio: 'pipe' })
    expect(String(execSpy.mock.calls[0][2]?.cwd)).toEndWith(path.basename(repoDir))
  })

  it('skips regeneration during dry run', () => {
    const repoDir = fs.mkdtempSync(path.join(os.tmpdir(), 'bump-version-'))
    tempDirs.push(repoDir)
    process.chdir(repoDir)
    fs.writeFileSync(path.join(repoDir, 'bun.lock'), '{}\n', 'utf8')

    const execSpy = vi.spyOn(childProcess, 'execFileSync').mockReturnValue(Buffer.from(''))

    const result = updateBunLock(true)

    expect(result.skipped).toBe(true)
    expect(result.reason).toBe('dry run')
    expect(result.path).toEndWith('bun.lock')
    expect(execSpy).not.toHaveBeenCalled()
  })
})

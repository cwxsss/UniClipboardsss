import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { afterAll, expect, it } from 'vitest'

const repoRoot = path.resolve(__dirname, '..', '..')
const srcTauriDir = path.join(repoRoot, 'src-tauri')
const targetTriple = execFileSync('rustc', ['-vV'], {
  cwd: repoRoot,
  encoding: 'utf8',
})
  .split('\n')
  .find(line => line.startsWith('host: '))
  ?.replace('host: ', '')
  .trim()

if (!targetTriple) {
  throw new Error('Failed to resolve Rust host target triple')
}

const stagedBinaryPath = path.join(
  srcTauriDir,
  'binaries',
  `uniclipboard-daemon-${targetTriple}${process.platform === 'win32' ? '.exe' : ''}`
)
const daemonBuildCacheBinaryPath = path.join(
  srcTauriDir,
  'target',
  'daemon-sidecar',
  targetTriple,
  'debug',
  `uniclipboard-daemon${process.platform === 'win32' ? '.exe' : ''}`
)
const backups: Array<{ original: string; backup: string }> = []

function moveOutOfTheWay(filePath: string) {
  if (!fs.existsSync(filePath)) {
    return
  }

  const backup = `${filePath}.bak-vitest`
  fs.rmSync(backup, { force: true })
  fs.renameSync(filePath, backup)
  backups.push({ original: filePath, backup })
}

afterAll(() => {
  while (backups.length > 0) {
    const entry = backups.pop()
    if (!entry) {
      continue
    }

    fs.rmSync(entry.original, { force: true })
    if (fs.existsSync(entry.backup)) {
      fs.renameSync(entry.backup, entry.original)
    }
  }
})

it('builds and stages the daemon sidecar when only the Tauri app is built', () => {
  moveOutOfTheWay(daemonBuildCacheBinaryPath)
  moveOutOfTheWay(stagedBinaryPath)

  execFileSync('cargo', ['build', '-p', 'uniclipboard', '--message-format', 'short'], {
    cwd: srcTauriDir,
    stdio: 'pipe',
  })

  expect(fs.existsSync(daemonBuildCacheBinaryPath)).toBe(true)
  expect(fs.existsSync(stagedBinaryPath)).toBe(true)
}, 120_000)

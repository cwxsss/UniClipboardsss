#!/usr/bin/env node

/* global process */

/**
 * Assemble Update Manifest Script
 *
 * Usage:
 *   node scripts/assemble-update-manifest.js \
 *     --version 0.1.0-alpha.1 \
 *     --artifacts-dir release-assets \
 *     --output updates/alpha.json \
 *     --base-url https://github.com/UniClipboard/UniClipboard/releases/download/v0.1.0-alpha.1
 *
 * Options:
 *   --version <ver>           Release version (required)
 *   --artifacts-dir <path>    Directory containing .sig files (required)
 *   --output <path>           Output JSON file path (required)
 *   --base-url <url>          GitHub release download base URL (required)
 *   --test                    Dry-run with mock data, output to stdout
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

// Parse command line arguments
function parseArgs() {
  const args = process.argv.slice(2)
  const options = {
    version: null,
    artifactsDir: null,
    output: null,
    baseUrl: null,
    test: false,
  }

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--version' && args[i + 1]) {
      options.version = args[i + 1]
      i++
    } else if (args[i] === '--artifacts-dir' && args[i + 1]) {
      options.artifactsDir = args[i + 1]
      i++
    } else if (args[i] === '--output' && args[i + 1]) {
      options.output = args[i + 1]
      i++
    } else if (args[i] === '--base-url' && args[i + 1]) {
      options.baseUrl = args[i + 1]
      i++
    } else if (args[i] === '--test') {
      options.test = true
    }
  }

  return options
}

/**
 * Determine platform from .sig filename.
 *
 * Mapping rules:
 *   - aarch64 + .app.tar.gz.sig  → darwin-aarch64
 *   - x64/x86_64 + .app.tar.gz.sig → darwin-x86_64
 *   - .AppImage.tar.gz.sig        → linux-x86_64
 *   - .nsis.zip.sig or .msi.zip.sig → windows-x86_64
 *
 * Returns null if no match.
 */
function detectPlatform(filename) {
  if (filename.endsWith('.app.tar.gz.sig')) {
    if (filename.includes('aarch64')) {
      return 'darwin-aarch64'
    }
    if (filename.includes('x64') || filename.includes('x86_64')) {
      return 'darwin-x86_64'
    }
    return null
  }

  if (filename.endsWith('.AppImage.tar.gz.sig')) {
    return 'linux-x86_64'
  }

  if (filename.endsWith('.nsis.zip.sig') || filename.endsWith('.msi.zip.sig')) {
    return 'windows-x86_64'
  }

  return null
}

/**
 * Scan artifacts directory for .sig files and build platform map.
 * @param {string} artifactsDir - Directory containing .sig files
 * @param {string} baseUrl - Base URL for download links
 * @param {boolean} [silent=false] - Suppress diagnostic output to stderr
 */
function scanArtifacts(artifactsDir, baseUrl, silent = false) {
  const platforms = {}

  const files = fs.readdirSync(artifactsDir)
  const sigFiles = files.filter(f => f.endsWith('.sig'))

  if (sigFiles.length === 0 && !silent) {
    process.stderr.write(`Warning: No .sig files found in ${artifactsDir}\n`)
  }

  for (const sigFile of sigFiles) {
    const platform = detectPlatform(sigFile)
    if (!platform) {
      if (!silent) {
        process.stderr.write(`Skipping unrecognized .sig file: ${sigFile}\n`)
      }
      continue
    }

    const sigPath = path.join(artifactsDir, sigFile)
    const signature = fs.readFileSync(sigPath, 'utf8').trim()

    // The artifact filename is the sig filename without the trailing .sig
    const artifactFilename = sigFile.slice(0, -4) // remove ".sig"
    const url = `${baseUrl}/${artifactFilename}`

    platforms[platform] = { signature, url }

    if (!silent) {
      process.stderr.write(`  [${platform}] ${sigFile} -> ${url}\n`)
    }
  }

  return platforms
}

/**
 * Assemble the combined update manifest JSON.
 */
function assembleManifest(version, platforms) {
  const pubDate = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z')

  return {
    version,
    notes: '',
    pub_date: pubDate,
    platforms,
  }
}

/**
 * Create mock .sig files in a temp directory for --test mode.
 */
function createMockArtifacts() {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'assemble-manifest-test-'))

  const mockFiles = {
    'uniclipboard_0.1.0-alpha.1_aarch64.app.tar.gz.sig':
      'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRVEFBQUFBQUFBQUFBQSttb2NrLXNpZ25hdHVyZS1hYXJjaDY0',
    'uniclipboard_0.1.0-alpha.1_x64.app.tar.gz.sig':
      'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRVEFBQUFBQUFBQUFBQSttb2NrLXNpZ25hdHVyZS14NjQ=',
    'uniclipboard_0.1.0-alpha.1_amd64.AppImage.tar.gz.sig':
      'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRVEFBQUFBQUFBQUFBQSttb2NrLXNpZ25hdHVyZS1saW51eA==',
    'uniclipboard_0.1.0-alpha.1_x64-setup.nsis.zip.sig':
      'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRVEFBQUFBQUFBQUFBQSttb2NrLXNpZ25hdHVyZS13aW5kb3dz',
  }

  for (const [filename, content] of Object.entries(mockFiles)) {
    fs.writeFileSync(path.join(tmpDir, filename), content, 'utf8')
  }

  return tmpDir
}

function main() {
  const options = parseArgs()

  if (options.test) {
    // --test mode: create mock data and output valid JSON to stdout only.
    // All diagnostic output is suppressed so that piped JSON parsing works cleanly
    // (e.g. node scripts/assemble-update-manifest.js --test 2>&1 | node -e "JSON.parse(d)").
    const version = options.version || '0.1.0-alpha.1'
    const baseUrl =
      options.baseUrl ||
      `https://github.com/UniClipboard/UniClipboard/releases/download/v${version}`

    const tmpDir = createMockArtifacts()

    try {
      const platforms = scanArtifacts(tmpDir, baseUrl, true)
      const manifest = assembleManifest(version, platforms)
      const json = JSON.stringify(manifest, null, 2)

      process.stdout.write(json + '\n')
    } finally {
      // Clean up temp dir
      fs.rmSync(tmpDir, { recursive: true, force: true })
    }

    return
  }

  // Normal mode: validate required args
  const missing = []
  if (!options.version) missing.push('--version')
  if (!options.artifactsDir) missing.push('--artifacts-dir')
  if (!options.output) missing.push('--output')
  if (!options.baseUrl) missing.push('--base-url')

  if (missing.length > 0) {
    process.stderr.write(`Error: Missing required arguments: ${missing.join(', ')}\n`)
    process.stderr.write(
      'Usage: node scripts/assemble-update-manifest.js --version <ver> --artifacts-dir <path> --output <path> --base-url <url>\n'
    )
    process.exit(1)
  }

  const artifactsDir = path.resolve(options.artifactsDir)

  if (!fs.existsSync(artifactsDir)) {
    process.stderr.write(`Error: Artifacts directory does not exist: ${artifactsDir}\n`)
    process.exit(1)
  }

  process.stderr.write(`Assembling update manifest for v${options.version}\n`)
  process.stderr.write(`Artifacts dir: ${artifactsDir}\n`)
  process.stderr.write(`Base URL: ${options.baseUrl}\n\n`)

  const platforms = scanArtifacts(artifactsDir, options.baseUrl)

  if (Object.keys(platforms).length === 0) {
    process.stderr.write('Error: No recognized platform artifacts found.\n')
    process.exit(1)
  }

  const manifest = assembleManifest(options.version, platforms)
  const json = JSON.stringify(manifest, null, 2)

  // Ensure output directory exists
  const outputPath = path.resolve(options.output)
  const outputDir = path.dirname(outputPath)
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true })
  }

  fs.writeFileSync(outputPath, json + '\n', 'utf8')

  process.stderr.write(`\nManifest written to: ${outputPath}\n`)
  process.stderr.write(`Platforms included: ${Object.keys(platforms).join(', ')}\n`)
}

main()

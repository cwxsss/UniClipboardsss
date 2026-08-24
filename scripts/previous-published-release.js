import fs from 'node:fs'
import process from 'node:process'
import { fileURLToPath } from 'node:url'
import { compare, prerelease, valid } from 'semver'

export function stripVersionTagPrefix(value) {
  return value.startsWith('v') ? value.slice(1) : value
}

export function compareVersions(aVersion, bVersion) {
  const a = valid(stripVersionTagPrefix(aVersion))
  const b = valid(stripVersionTagPrefix(bVersion))
  if (!a || !b) {
    throw new Error(`Invalid semver version: ${aVersion} or ${bVersion}`)
  }
  return compare(a, b)
}

function normalizeRelease(release) {
  return {
    tagName: release.tagName ?? release.tag_name,
    isDraft: release.isDraft ?? release.draft ?? false,
    isPrerelease: release.isPrerelease ?? release.prerelease ?? false,
    publishedAt: release.publishedAt ?? release.published_at ?? null,
    version: stripVersionTagPrefix(release.tagName ?? release.tag_name),
  }
}

export function selectPreviousPublishedRelease(releases, currentVersion) {
  const normalizedCurrentVersion = stripVersionTagPrefix(currentVersion)
  const currentSemver = valid(normalizedCurrentVersion)
  if (!currentSemver) {
    throw new Error(`Invalid current semver version: ${currentVersion}`)
  }
  const currentIsStable = prerelease(currentSemver) === null

  const candidates = releases
    .filter(release => !release.isDraft)
    .filter(release => Boolean(release.publishedAt))
    .map(normalizeRelease)
    .filter(release => Boolean(valid(release.version)))
    .filter(release => compareVersions(release.version, normalizedCurrentVersion) < 0)
    .filter(release => !currentIsStable || prerelease(valid(release.version)) === null)
    .sort((left, right) => compareVersions(right.version, left.version))

  return candidates[0] ?? null
}

export async function fetchPublishedReleases(repo, token, fetchImpl = fetch) {
  const releases = []
  let page = 1

  while (true) {
    const response = await fetchImpl(
      `https://api.github.com/repos/${repo}/releases?per_page=100&page=${page}`,
      {
        headers: {
          Accept: 'application/vnd.github+json',
          Authorization: token ? `Bearer ${token}` : undefined,
          'User-Agent': 'uniclipboard-release-history',
        },
      }
    )

    if (!response.ok) {
      throw new Error(
        `Failed to fetch releases for ${repo}: ${response.status} ${response.statusText}`
      )
    }

    const pageItems = await response.json()
    releases.push(...pageItems.map(normalizeRelease))

    if (pageItems.length < 100) {
      break
    }

    page += 1
  }

  return releases
}

function parseArgs(argv) {
  const args = {}

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index]
    const next = argv[index + 1]

    if (!current.startsWith('--')) {
      continue
    }

    const key = current.slice(2)
    args[key] = next
    index += 1
  }

  return args
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  const repo = args.repo || process.env.GITHUB_REPOSITORY
  const currentVersion = args['current-version']

  if (!repo) {
    throw new Error('Missing --repo or GITHUB_REPOSITORY')
  }

  if (!currentVersion) {
    throw new Error('Missing --current-version')
  }

  const token =
    process.env.GITHUB_TOKEN || process.env.GH_TOKEN || process.env.REPO_BOT_TOKEN || undefined
  const releases = await fetchPublishedReleases(repo, token)
  const previousRelease = selectPreviousPublishedRelease(releases, currentVersion)
  const payload = {
    previousTag: previousRelease?.tagName ?? '',
    previousVersion: previousRelease?.version ?? '',
  }

  if (args['github-output']) {
    const lines = [
      `previous_tag=${payload.previousTag}`,
      `previous_version=${payload.previousVersion}`,
    ]
    fs.appendFileSync(args['github-output'], `${lines.join('\n')}\n`)
    return
  }

  process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch(error => {
    console.error(error instanceof Error ? error.message : String(error))
    process.exitCode = 1
  })
}

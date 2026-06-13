#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { readdir, stat } from 'node:fs/promises'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  artifactKindFromPath,
  buildMacosReleaseArtifactChecks,
  formatArtifactPath,
  formatReleaseArtifactValidationReport,
  sanitizeReleaseValidationOutput,
  selectLatestReleaseArtifacts
} from './lib/macos-release-artifact-validation.mjs'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const releaseDir = join(repoRoot, 'apps', 'desktop', 'release')

async function main() {
  if (process.platform !== 'darwin') {
    throw new Error('macOS release artifact validation must run on macOS.')
  }

  const artifactPaths =
    process.argv.slice(2).length > 0
      ? process.argv.slice(2).map((path) => resolve(path))
      : (await discoverLatestArtifacts()).map((artifact) => artifact.path)

  if (artifactPaths.length === 0) {
    throw new Error('No .app or .dmg release artifacts found under apps/desktop/release.')
  }

  let allPassed = true
  for (const artifactPath of artifactPaths) {
    const passed = validateArtifact(artifactPath)
    allPassed = allPassed && passed
  }

  process.exit(allPassed ? 0 : 1)
}

async function discoverLatestArtifacts() {
  if (!existsSync(releaseDir)) {
    return []
  }

  const candidates = await collectArtifacts(releaseDir)
  return selectLatestReleaseArtifacts(candidates)
}

async function collectArtifacts(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const candidates = []

  for (const entry of entries) {
    const path = join(directory, entry.name)
    if (entry.isDirectory() && entry.name.endsWith('.app')) {
      const info = await stat(path)
      candidates.push({ path, kind: 'app', mtimeMs: info.mtimeMs })
      continue
    }

    if (entry.isFile() && entry.name.endsWith('.dmg')) {
      const info = await stat(path)
      candidates.push({ path, kind: 'dmg', mtimeMs: info.mtimeMs })
      continue
    }

    if (entry.isDirectory()) {
      candidates.push(...(await collectArtifacts(path)))
    }
  }

  return candidates
}

function validateArtifact(artifactPath) {
  if (!existsSync(artifactPath)) {
    console.error(`macos-release-artifact: FAIL ${formatArtifactPath(artifactPath, context())}`)
    console.error('[fail] artifact exists')
    return false
  }

  const kind = artifactKindFromPath(artifactPath)
  if (!kind) {
    console.error(`macos-release-artifact: FAIL ${formatArtifactPath(artifactPath, context())}`)
    console.error('[fail] unsupported artifact type; expected .app or .dmg')
    return false
  }

  const checks = buildMacosReleaseArtifactChecks(artifactPath)
  const results = checks.map(runCheck)
  const artifactLabel = formatArtifactPath(artifactPath, context())
  const report = formatReleaseArtifactValidationReport({ artifactLabel, results })
  if (results.every((result) => result.ok)) {
    console.log(report)
    return true
  }

  console.error(report)
  return false
}

function runCheck(check) {
  const result = spawnSync(check.command, check.args, {
    encoding: 'utf8'
  })
  const output = sanitizeReleaseValidationOutput(
    [result.stdout, result.stderr].filter(Boolean).join('\n'),
    context()
  )

  return {
    label: check.label,
    ok: result.status === 0,
    output
  }
}

function context() {
  return {
    repoRoot,
    homeDir: homedir()
  }
}

main().catch((error) => {
  console.error(`macos-release-artifact: FAIL (${error?.message ?? 'unexpected error'})`)
  process.exit(1)
})

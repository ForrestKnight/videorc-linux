import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

import {
  artifactKindFromPath,
  buildMacosReleaseArtifactChecks,
  formatArtifactPath,
  formatReleaseArtifactValidationReport,
  sanitizeReleaseValidationOutput,
  selectLatestReleaseArtifacts
} from './macos-release-artifact-validation.mjs'

describe('artifactKindFromPath', () => {
  it('recognizes app bundles and DMGs only', () => {
    assert.equal(artifactKindFromPath('/tmp/Videorc.app'), 'app')
    assert.equal(artifactKindFromPath('/tmp/Videorc.dmg'), 'dmg')
    assert.equal(artifactKindFromPath('/tmp/Videorc.dmg.blockmap'), null)
  })
})

describe('buildMacosReleaseArtifactChecks', () => {
  it('uses strict app validation commands', () => {
    const checks = buildMacosReleaseArtifactChecks('/release/Videorc.app')

    assert.deepEqual(
      checks.map((check) => check.label),
      ['codesign verify', 'codesign display', 'Gatekeeper assess', 'stapler validate']
    )
    assert.deepEqual(checks[0].args, [
      '--verify',
      '--deep',
      '--strict',
      '--verbose=2',
      '/release/Videorc.app'
    ])
    assert.deepEqual(checks[2].args, [
      '--assess',
      '--type',
      'execute',
      '--verbose',
      '/release/Videorc.app'
    ])
  })

  it('uses primary-signature assessment for DMGs', () => {
    const checks = buildMacosReleaseArtifactChecks('/release/Videorc.dmg')

    assert.deepEqual(checks[0].args, ['--verify', '--verbose=2', '/release/Videorc.dmg'])
    assert.deepEqual(checks[2].args, [
      '--assess',
      '--type',
      'open',
      '--context',
      'context:primary-signature',
      '--verbose',
      '/release/Videorc.dmg'
    ])
  })
})

describe('selectLatestReleaseArtifacts', () => {
  it('selects the newest app and newest DMG, ignoring unsupported files', () => {
    const selected = selectLatestReleaseArtifacts([
      { path: '/release/old/Videorc.app', mtimeMs: 10 },
      { path: '/release/new/Videorc.app', mtimeMs: 20 },
      { path: '/release/Videorc-old.dmg', mtimeMs: 15 },
      { path: '/release/Videorc-new.dmg', mtimeMs: 25 },
      { path: '/release/Videorc-new.dmg.blockmap', mtimeMs: 30 }
    ])

    assert.deepEqual(
      selected.map((artifact) => artifact.path),
      ['/release/new/Videorc.app', '/release/Videorc-new.dmg']
    )
  })
})

describe('release artifact report redaction', () => {
  it('formats repo-relative artifact paths', () => {
    assert.equal(
      formatArtifactPath('/repo/apps/desktop/release/mac-arm64/Videorc.app', {
        repoRoot: '/repo',
        homeDir: '/Users/orcdev'
      }),
      'apps/desktop/release/mac-arm64/Videorc.app'
    )
  })

  it('redacts home and repo paths from command output', () => {
    const output = sanitizeReleaseValidationOutput(
      '/repo/apps/desktop/release/mac-arm64/Videorc.app\n/Users/orcdev/Library/secret',
      {
        repoRoot: '/repo',
        homeDir: '/Users/orcdev'
      }
    )

    assert.equal(output, '<repo>/apps/desktop/release/mac-arm64/Videorc.app\n<home>/Library/secret')
  })

  it('includes only failing command excerpts', () => {
    const report = formatReleaseArtifactValidationReport({
      artifactLabel: 'apps/desktop/release/Videorc.dmg',
      results: [
        { label: 'codesign verify', ok: true, output: 'unused success output' },
        { label: 'Gatekeeper assess', ok: false, output: 'rejected\n/path detail' }
      ]
    })

    assert.match(report, /macos-release-artifact: FAIL apps\/desktop\/release\/Videorc\.dmg/)
    assert.match(report, /\[ok\] codesign verify/)
    assert.match(report, /\[fail\] Gatekeeper assess/)
    assert.match(report, /  rejected/)
    assert.doesNotMatch(report, /unused success output/)
  })
})

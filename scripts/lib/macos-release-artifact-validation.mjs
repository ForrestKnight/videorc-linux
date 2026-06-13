import { basename, relative } from 'node:path'

export function artifactKindFromPath(path) {
  if (String(path).endsWith('.app')) {
    return 'app'
  }
  if (String(path).endsWith('.dmg')) {
    return 'dmg'
  }
  return null
}

export function buildMacosReleaseArtifactChecks(path) {
  const kind = artifactKindFromPath(path)
  if (!kind) {
    throw new Error(`Unsupported macOS release artifact: ${path}`)
  }

  if (kind === 'app') {
    return [
      {
        id: 'codesign-verify',
        label: 'codesign verify',
        command: 'codesign',
        args: ['--verify', '--deep', '--strict', '--verbose=2', path]
      },
      {
        id: 'codesign-display',
        label: 'codesign display',
        command: 'codesign',
        args: ['-dv', '--verbose=4', path]
      },
      {
        id: 'spctl-assess',
        label: 'Gatekeeper assess',
        command: 'spctl',
        args: ['--assess', '--type', 'execute', '--verbose', path]
      },
      {
        id: 'stapler-validate',
        label: 'stapler validate',
        command: 'xcrun',
        args: ['stapler', 'validate', path]
      }
    ]
  }

  return [
    {
      id: 'codesign-verify',
      label: 'codesign verify',
      command: 'codesign',
      args: ['--verify', '--verbose=2', path]
    },
    {
      id: 'codesign-display',
      label: 'codesign display',
      command: 'codesign',
      args: ['-dv', '--verbose=4', path]
    },
    {
      id: 'spctl-assess',
      label: 'Gatekeeper assess',
      command: 'spctl',
      args: [
        '--assess',
        '--type',
        'open',
        '--context',
        'context:primary-signature',
        '--verbose',
        path
      ]
    },
    {
      id: 'stapler-validate',
      label: 'stapler validate',
      command: 'xcrun',
      args: ['stapler', 'validate', path]
    }
  ]
}

export function selectLatestReleaseArtifacts(candidates) {
  const latestByKind = new Map()

  for (const candidate of candidates) {
    const kind = candidate.kind ?? artifactKindFromPath(candidate.path)
    if (!kind) {
      continue
    }

    const previous = latestByKind.get(kind)
    if (!previous || Number(candidate.mtimeMs ?? 0) > Number(previous.mtimeMs ?? 0)) {
      latestByKind.set(kind, { ...candidate, kind })
    }
  }

  return ['app', 'dmg'].map((kind) => latestByKind.get(kind)).filter(Boolean)
}

export function formatArtifactPath(path, { repoRoot, homeDir } = {}) {
  const raw = String(path)
  if (repoRoot) {
    const rel = relative(repoRoot, raw)
    if (rel && !rel.startsWith('..')) {
      return rel
    }
  }

  if (homeDir && raw.startsWith(homeDir)) {
    return `<home>/${basename(raw)}`
  }

  return `<external>/${basename(raw)}`
}

export function sanitizeReleaseValidationOutput(text, { repoRoot, homeDir } = {}) {
  let output = String(text ?? '')
  if (repoRoot) {
    output = output.split(repoRoot).join('<repo>')
  }
  if (homeDir) {
    output = output.split(homeDir).join('<home>')
  }
  return output
}

export function formatReleaseArtifactValidationReport({ artifactLabel, results }) {
  const ok = results.every((result) => result.ok)
  const lines = [`macos-release-artifact: ${ok ? 'PASS' : 'FAIL'} ${artifactLabel}`]

  for (const result of results) {
    const mark = result.ok ? 'ok' : 'fail'
    lines.push(`[${mark}] ${result.label}`)
    if (!result.ok && result.output) {
      lines.push(indentExcerpt(result.output))
    }
  }

  return lines.join('\n')
}

function indentExcerpt(text) {
  return String(text)
    .trim()
    .split(/\r?\n/)
    .slice(0, 12)
    .map((line) => `  ${line}`)
    .join('\n')
}

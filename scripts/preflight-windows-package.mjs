// Asserts the Windows packaging inputs exist before electron-builder runs,
// because electron-builder's behavior on a missing extraResources source is
// not a reliable loud failure. Run by package:desktop:windows.

import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')

const inputs = [
  {
    path: join(repoRoot, 'target', 'release', 'videorc-backend.exe'),
    remedy: 'pnpm package:backend'
  },
  {
    path: join(repoRoot, 'vendor', 'ffmpeg', 'windows-x64', 'bin', 'ffmpeg.exe'),
    remedy: 'pnpm ffmpeg:fetch:windows'
  }
]

const missing = inputs.filter((input) => !existsSync(input.path))
for (const input of missing) {
  console.error(
    `preflight-windows-package: MISSING ${input.path} — produce it with: ${input.remedy}`
  )
}
if (missing.length > 0) {
  process.exit(1)
}
console.log('preflight-windows-package: all Windows packaging inputs present.')

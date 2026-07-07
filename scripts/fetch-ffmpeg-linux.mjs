// Fetches the pinned prebuilt LGPL linux64 FFmpeg (BtbN build) and lays it out
// as vendor/ffmpeg/linux-x64/{bin/ffmpeg,bin/ffprobe,LICENSE.txt,SOURCE.txt} —
// the shape apps/desktop/electron-builder.yml bundles for the Linux target.
// The pin (URL + sha256) lives in vendor/ffmpeg/linux-pin.json and is the
// committed reproducibility record; the payload itself is gitignored.
//
// Mirrors scripts/fetch-ffmpeg-windows.mjs and its LGPL discipline: never pin
// an asset whose name lacks "lgpl". SOURCE.txt records the exact upstream URL
// (the LGPL source-offer breadcrumb that ships inside the AppImage).
//
// The BtbN "latest" URL is a moving target, so the sha256 IS the pin: a
// rebuilt upstream asset fails the checksum and forces a deliberate pin
// update, exactly like the Windows fetch.
//
// Usage: node scripts/fetch-ffmpeg-linux.mjs [--force]

import { createHash } from 'node:crypto'
import { createWriteStream } from 'node:fs'
import { chmod, copyFile, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { Readable } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import { fileURLToPath } from 'node:url'
import { execFileSync } from 'node:child_process'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const pinPath = join(repoRoot, 'vendor', 'ffmpeg', 'linux-pin.json')
const downloadPath = join(repoRoot, 'vendor', 'ffmpeg', '_build', 'linux-download.tar.xz')
const extractDir = join(repoRoot, 'vendor', 'ffmpeg', '_build', 'linux-extract')
const outputDir = join(repoRoot, 'vendor', 'ffmpeg', 'linux-x64')
const force = process.argv.includes('--force')

function fail(message) {
  console.error(`fetch-ffmpeg-linux: ${message}`)
  process.exit(1)
}

async function fileExists(path) {
  try {
    await stat(path)
    return true
  } catch {
    return false
  }
}

async function sha256Of(path) {
  const hash = createHash('sha256')
  hash.update(await readFile(path))
  return hash.digest('hex')
}

const pin = JSON.parse(await readFile(pinPath, 'utf8'))
if (!pin.url || !pin.sha256) {
  fail(`${pinPath} must contain { url, sha256 }`)
}
if (!/lgpl/.test(pin.url)) {
  fail(`pinned URL is not an LGPL build: ${pin.url} (LGPL-only is the repo's ffmpeg policy)`)
}

const ffmpegBin = join(outputDir, 'bin', 'ffmpeg')
const ffprobeBin = join(outputDir, 'bin', 'ffprobe')
const sourceTxt = join(outputDir, 'SOURCE.txt')
if (!force && (await fileExists(ffmpegBin)) && (await fileExists(sourceTxt))) {
  const recorded = await readFile(sourceTxt, 'utf8')
  if (recorded.includes(pin.sha256)) {
    console.log(
      `Pinned FFmpeg already present at ${ffmpegBin} — skipping download (use --force to re-fetch).`
    )
    process.exit(0)
  }
}

// Reuse a previously downloaded tarball when its checksum matches the pin.
let haveTarball = false
if (!force && (await fileExists(downloadPath))) {
  haveTarball = (await sha256Of(downloadPath)) === pin.sha256
}
if (!haveTarball) {
  console.log(`Downloading ${pin.url}`)
  await mkdir(dirname(downloadPath), { recursive: true })
  const response = await fetch(pin.url, { redirect: 'follow' })
  if (!response.ok || !response.body) {
    fail(`download failed: HTTP ${response.status} for ${pin.url}`)
  }
  await pipeline(Readable.fromWeb(response.body), createWriteStream(downloadPath))
}

const actualSha = await sha256Of(downloadPath)
if (actualSha !== pin.sha256) {
  fail(
    `checksum mismatch for ${downloadPath}\n  expected: ${pin.sha256}\n  actual:   ${actualSha}\n` +
      'Refusing to install. The BtbN "latest" asset was rebuilt upstream — verify it is still an ' +
      'LGPL build, then update vendor/ffmpeg/linux-pin.json deliberately and re-run.'
  )
}

await rm(extractDir, { recursive: true, force: true })
await mkdir(extractDir, { recursive: true })
execFileSync('tar', ['-xJf', downloadPath, '-C', extractDir], { stdio: 'inherit' })

const extracted = (await readdir(extractDir)).filter((name) => name.startsWith('ffmpeg-'))
if (extracted.length !== 1) {
  fail(`expected one ffmpeg-* dir inside the tarball, found: ${extracted.join(', ') || '(none)'}`)
}
const tarRoot = join(extractDir, extracted[0])

await rm(outputDir, { recursive: true, force: true })
await mkdir(join(outputDir, 'bin'), { recursive: true })
await copyFile(join(tarRoot, 'bin', 'ffmpeg'), ffmpegBin).catch(() =>
  fail(`tarball layout drift: ${extracted[0]}/bin/ffmpeg not found`)
)
await copyFile(join(tarRoot, 'bin', 'ffprobe'), ffprobeBin).catch(() =>
  fail(`tarball layout drift: ${extracted[0]}/bin/ffprobe not found`)
)
await chmod(ffmpegBin, 0o755)
await chmod(ffprobeBin, 0o755)
await copyFile(join(tarRoot, 'LICENSE.txt'), join(outputDir, 'LICENSE.txt')).catch(() =>
  fail(`tarball layout drift: ${extracted[0]}/LICENSE.txt not found`)
)
await writeFile(
  sourceTxt,
  [
    'Prebuilt FFmpeg (LGPL) for the Videorc Linux bundle.',
    `URL: ${pin.url}`,
    `SHA256: ${pin.sha256}`,
    `Fetched: ${new Date().toISOString()}`,
    'Corresponding source: https://github.com/BtbN/FFmpeg-Builds (LGPL build; see the repo for sources).',
    ''
  ].join('\n')
)

if (!(await fileExists(ffmpegBin)) || !(await fileExists(ffprobeBin))) {
  fail(`assembly finished but ${ffmpegBin} or ${ffprobeBin} is missing`)
}
console.log(`FFmpeg (linux64 LGPL) ready at ${outputDir}`)

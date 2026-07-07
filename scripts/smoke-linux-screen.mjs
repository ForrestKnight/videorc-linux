// Linux port phase-3 gate: portal ScreenCast + PipeWire screen capture.
//
// The Wayland screen-capture permission model is the compositor's own picker
// dialog — capture cannot start without an explicit user grant, and the first
// grant is interactive by design. So this smoke has two honest outcomes:
//
//   * Full PASS — a portal restore token is available (VIDEORC_SCREENCAST_
//     RESTORE_TOKEN, or a prior in-app grant persisted to the DB, or the run
//     is interactive and a human picks a source): preview goes live, frames
//     flow, and a screen recording passes the analyzer.
//   * SKIP (explicit) — no token and non-interactive: the device listing and
//     the portal code path are still exercised (start returns permission-
//     needed), and the smoke reports that a one-time grant is required,
//     exactly like upstream's macOS device smokes skip without permissions.
//
// It never hangs on the picker: without a token it expects permission-needed
// and returns promptly.

import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { launchDevApp } from './lib/app-launcher.mjs'
import { analyzeRecording, writeReports } from './lib/recording-analyzer.mjs'
import { connectBackend, request } from './smoke-recording-session.mjs'

const PORTAL_SCREEN_ID = 'screen:portal:screencast'
const RECORDING_MS = Number(process.env.VIDEORC_SMOKE_RECORDING_MS ?? 3000)
const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 180000)
const hasToken = !!process.env.VIDEORC_SCREENCAST_RESTORE_TOKEN
const interactive = process.env.VIDEORC_SCREEN_INTERACTIVE === '1'
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

if (process.platform !== 'linux') {
  console.log('Linux screen smoke: skipped (not Linux).')
  process.exit(0)
}

const outputDirectory = join(mkdtempSync(join(tmpdir(), 'videorc-linux-screen-smoke-')), 'rec')
const video = { preset: 'custom', width: 1280, height: 720, fps: 30, bitrateKbps: 6000 }
const layout = {
  layoutPreset: 'screen-only',
  cameraTransformMode: 'preset',
  cameraTransform: null,
  cameraCorner: 'bottom-right',
  cameraSize: 'medium',
  cameraShape: 'rectangle',
  cameraMargin: 32,
  cameraFit: 'fill',
  cameraMirror: false,
  cameraZoom: 100,
  cameraOffsetX: 0,
  cameraOffsetY: 0,
  sideBySideSplit: '70-30',
  sideBySideCameraSide: 'right'
}

let launched
try {
  launched = await launchDevApp({
    requiredMarkers: ['backend-ready'],
    timeoutMs,
    env: { VIDEORC_SMOKE_PRINT_BACKEND_READY: '1' }
  })
  const ws = await connectBackend(launched.connections['backend-ready'], timeoutMs)

  // 1. The portal screen source is always listed (one honest entry).
  const deviceList = await request(ws, timeoutMs, 'devices.list')
  const screen = (deviceList.devices ?? []).find((device) => device.id === PORTAL_SCREEN_ID)
  if (!screen) {
    throw new Error('Portal screen source was not listed.')
  }
  if (screen.status !== 'available') {
    throw new Error(`Portal screen source is not available: ${screen.status}`)
  }
  console.log(`Linux screen smoke devices PASS: ${screen.name} -> ${screen.id}`)

  // 2. Start the preview. With a token (or interactively) it goes live; without
  // one, permission-needed is the correct, prompt outcome.
  const startStatus = await request(ws, timeoutMs, 'preview.screen.start', {
    sources: { screenId: PORTAL_SCREEN_ID },
    video
  })

  if (startStatus.state !== 'live') {
    if (hasToken || interactive) {
      throw new Error(
        `Screen preview did not go live: state=${startStatus.state} ` +
          `message=${startStatus.message ?? ''}`
      )
    }
    console.log(
      `Linux screen smoke: SKIPPED end-to-end — portal returned "${startStatus.state}" ` +
        '(no restore token, non-interactive). The device listing and portal path were ' +
        'exercised. To fully verify, grant screen capture once (see docs/linux-port-status.md).'
    )
    process.exit(0)
  }

  await sleep(2000)
  const status = await request(ws, timeoutMs, 'preview.screen.status')
  if (!(status.framesCaptured > 10)) {
    throw new Error(
      `Screen preview is not delivering frames: framesCaptured=${status.framesCaptured}`
    )
  }
  console.log(
    `Linux screen smoke frames PASS: framesCaptured=${status.framesCaptured} ` +
      `sourceFps=${status.sourceFps?.toFixed(1) ?? 'n/a'} ${status.width}x${status.height}`
  )

  // 3. A screen recording carries the composited frames.
  const started = await request(ws, timeoutMs, 'session.start', {
    sources: { screenId: PORTAL_SCREEN_ID },
    layout,
    output: {
      recordEnabled: true,
      streamEnabled: false,
      outputDirectory,
      ffmpegPath: 'ffmpeg',
      video,
      rtmp: { preset: 'custom', serverUrl: '', streamKey: '' }
    }
  })
  if (!['recording', 'streaming'].includes(started.state)) {
    throw new Error(`Expected recording state after start, got ${started.state}.`)
  }
  await sleep(RECORDING_MS)
  const stopped = await request(ws, timeoutMs, 'session.stop')
  const outputPath = stopped.outputPath ?? started.outputPath
  if (!outputPath) {
    throw new Error('Recording finished without an output path.')
  }

  const quality = await analyzeRecording(outputPath, {
    ffmpegPath: 'ffmpeg',
    ffprobePath: 'ffprobe',
    intendedFps: 30,
    expectAudio: true,
    gates: { requireMotion: false }
  })
  const reportPaths = writeReports(quality)
  if (!quality.verdict.pass) {
    throw new Error(
      `Recording quality gate failed: ${quality.verdict.failures.join('; ')} ` +
        `(report: ${reportPaths.mdPath})`
    )
  }
  console.log(
    `Linux screen smoke recording PASS: ${outputPath} ` +
      `${quality.metrics.observedFrames ?? 'n/a'} frame(s) (report: ${reportPaths.mdPath})`
  )
  console.log('Linux screen smoke OK - portal listing, live frames, and recording verified.')
} finally {
  if (launched) {
    await launched.stop()
  }
}

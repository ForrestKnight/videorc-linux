// Linux port phase-2 gate: V4L2 camera capture end to end.
//
// Drives the real backend over WS: the camera must enumerate with a
// camera:v4l2-native: id, preview.camera.start must go live and capture
// frames, and a camera-only recording must pass the analyzer gates. Needs a
// V4L2 camera that actually delivers frames (e.g. a Cam Link with an active
// HDMI source, or any UVC webcam); when none is present the smoke SKIPS
// explicitly rather than passing vacuously.

import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { launchDevApp } from './lib/app-launcher.mjs'
import { analyzeRecording, writeReports } from './lib/recording-analyzer.mjs'
import { connectBackend, request } from './smoke-recording-session.mjs'

const RECORDING_MS = Number(process.env.VIDEORC_SMOKE_RECORDING_MS ?? 3000)
const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 120000)
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

if (process.platform !== 'linux') {
  console.log('Linux camera smoke: skipped (not Linux).')
  process.exit(0)
}

const outputDirectory = join(mkdtempSync(join(tmpdir(), 'videorc-linux-camera-smoke-')), 'rec')

const layout = {
  layoutPreset: 'camera-only',
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
const video = { preset: 'custom', width: 1280, height: 720, fps: 30, bitrateKbps: 4000 }

let launched
try {
  launched = await launchDevApp({
    requiredMarkers: ['backend-ready'],
    timeoutMs,
    env: { VIDEORC_SMOKE_PRINT_BACKEND_READY: '1' }
  })
  const ws = await connectBackend(launched.connections['backend-ready'], timeoutMs)

  const deviceList = await request(ws, timeoutMs, 'devices.list')
  const camera = (deviceList.devices ?? []).find(
    (device) => device.kind === 'camera' && device.id.startsWith('camera:v4l2-native:')
  )
  if (!camera) {
    console.log('Linux camera smoke: SKIPPED — no V4L2 camera enumerated on this machine.')
    process.exit(0)
  }
  if (camera.status !== 'available') {
    throw new Error(`Camera is not available: ${camera.status} (${camera.detail ?? ''})`)
  }
  console.log(`Linux camera smoke devices PASS: ${camera.name} -> ${camera.id}`)

  const startStatus = await request(ws, timeoutMs, 'preview.camera.start', {
    sources: { cameraId: camera.id, testPattern: false },
    layout,
    video
  })
  if (startStatus.state !== 'live') {
    throw new Error(
      `Camera preview did not go live: state=${startStatus.state} message=${startStatus.message ?? ''}`
    )
  }
  console.log(
    `Linux camera smoke preview live: capture ${startStatus.actualWidth}x${startStatus.actualHeight} ` +
      `(requested ${startStatus.requestedWidth}x${startStatus.requestedHeight})`
  )

  await sleep(2000)
  const status = await request(ws, timeoutMs, 'preview.camera.status')
  if (!(status.framesCaptured > 10)) {
    throw new Error(
      `Camera preview is not delivering frames: framesCaptured=${status.framesCaptured} ` +
        `state=${status.state}. Is the capture source (HDMI feed) active?`
    )
  }
  console.log(
    `Linux camera smoke frames PASS: framesCaptured=${status.framesCaptured} ` +
      `sourceFps=${status.sourceFps?.toFixed(1) ?? 'n/a'}`
  )

  const started = await request(ws, timeoutMs, 'session.start', {
    sources: { cameraId: camera.id, testPattern: false },
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
    // Whatever the HDMI source shows may legitimately be static.
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
    `Linux camera smoke recording PASS: ${outputPath} ` +
      `${quality.metrics.observedFrames ?? 'n/a'} frame(s) ` +
      `(report: ${reportPaths.mdPath})`
  )
  console.log('Linux camera smoke OK - enumeration, live preview frames, and recording verified.')
} finally {
  if (launched) {
    await launched.stop()
  }
}

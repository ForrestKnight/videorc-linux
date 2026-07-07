// Linux port phase-4 capstone: the CPU compositor blends multiple real
// sources into one recording.
//
// Drives a screen + camera + microphone session in the screen-camera layout:
// the portal screen and the V4L2 camera composite into one frame while the
// PipeWire mic feeds audio. The finished artifact must carry composited video
// and a real (non-silent) audio track.
//
// Interactive-or-skip, like smoke:linux-screen: needs a portal grant (a
// restore token via VIDEORC_SCREENCAST_RESTORE_TOKEN, or VIDEORC_SCREEN_
// INTERACTIVE=1 with a human picking a source) and a V4L2 camera that
// delivers frames. Skips explicitly otherwise — never hangs.

import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { execFileSync, spawn } from 'node:child_process'
import { join } from 'node:path'

import { launchDevApp } from './lib/app-launcher.mjs'
import { analyzeRecording, writeReports } from './lib/recording-analyzer.mjs'
import { connectBackend, request } from './smoke-recording-session.mjs'

const PORTAL_SCREEN_ID = 'screen:portal:screencast'
const SINK_NAME = 'videorc_studio_sink'
const MIC_NAME = 'videorc_studio_mic'
const MIC_DESCRIPTION = 'VideorcStudioMic'
const RECORDING_MS = Number(process.env.VIDEORC_SMOKE_RECORDING_MS ?? 4000)
const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 180000)
const interactive = process.env.VIDEORC_SCREEN_INTERACTIVE === '1'
const hasToken = !!process.env.VIDEORC_SCREENCAST_RESTORE_TOKEN
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

if (process.platform !== 'linux') {
  console.log('Linux studio smoke: skipped (not Linux).')
  process.exit(0)
}

const scratch = mkdtempSync(join(tmpdir(), 'videorc-linux-studio-smoke-'))
const outputDirectory = join(scratch, 'rec')
const video = { preset: 'custom', width: 1280, height: 720, fps: 30, bitrateKbps: 6000 }
const moduleIds = []
let toneProcess
let launched

function pactl(...args) {
  return execFileSync('pactl', args, { encoding: 'utf8' }).trim()
}
function cleanup() {
  if (toneProcess && toneProcess.exitCode == null) toneProcess.kill('SIGKILL')
  for (const id of moduleIds.reverse()) {
    try {
      pactl('unload-module', id)
    } catch {
      /* already gone */
    }
  }
}

try {
  // Virtual mic carrying a tone, so the audio assertion needs no hardware.
  moduleIds.push(pactl('load-module', 'module-null-sink', `sink_name=${SINK_NAME}`))
  moduleIds.push(
    pactl(
      'load-module',
      'module-remap-source',
      `master=${SINK_NAME}.monitor`,
      `source_name=${MIC_NAME}`,
      `source_properties=device.description=${MIC_DESCRIPTION}`
    )
  )
  const tonePath = join(scratch, 'tone.wav')
  execFileSync('ffmpeg', [
    '-y',
    '-v',
    'quiet',
    '-f',
    'lavfi',
    '-i',
    'sine=frequency=440:sample_rate=48000:duration=120',
    tonePath
  ])
  toneProcess = spawn('paplay', ['--device', SINK_NAME, tonePath], { stdio: 'ignore' })

  launched = await launchDevApp({
    requiredMarkers: ['backend-ready'],
    timeoutMs,
    env: { VIDEORC_SMOKE_PRINT_BACKEND_READY: '1' }
  })
  const ws = await connectBackend(launched.connections['backend-ready'], timeoutMs)

  const deviceList = await request(ws, timeoutMs, 'devices.list')
  const devices = deviceList.devices ?? []
  const camera = devices.find(
    (d) => d.kind === 'camera' && d.id.startsWith('camera:v4l2-native:') && d.status === 'available'
  )
  const mic = devices.find((d) => d.kind === 'microphone' && d.name.includes(MIC_DESCRIPTION))
  if (!camera) {
    console.log('Linux studio smoke: SKIPPED — no V4L2 camera delivering frames.')
    process.exit(0)
  }
  if (!mic) {
    throw new Error('Studio virtual mic was not listed.')
  }

  // Bring the screen preview up first: without a token this returns
  // permission-needed and we skip (non-interactive) rather than hang.
  const screenStatus = await request(ws, timeoutMs, 'preview.screen.start', {
    sources: { screenId: PORTAL_SCREEN_ID },
    video
  })
  if (screenStatus.state !== 'live') {
    if (hasToken || interactive) {
      throw new Error(`Screen preview did not go live: ${screenStatus.state}`)
    }
    console.log(
      `Linux studio smoke: SKIPPED — screen returned "${screenStatus.state}" (no grant, ` +
        'non-interactive). Composition of synthetic multi-source layouts is covered by smoke:dev.'
    )
    process.exit(0)
  }

  const layout = {
    layoutPreset: 'screen-camera',
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
  const sources = { screenId: PORTAL_SCREEN_ID, cameraId: camera.id, microphoneId: mic.id }

  const started = await request(ws, timeoutMs, 'session.start', {
    sources,
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
  if (quality.metrics.longestSilenceMs == null) {
    throw new Error('Composite recording has no analyzable audio track.')
  }
  console.log(
    `Linux studio smoke PASS: screen+camera+mic composited into ${outputPath} — ` +
      `${quality.metrics.observedFrames ?? 'n/a'} frame(s), ` +
      `silence=${quality.metrics.longestSilenceMs.toFixed(0)}ms ` +
      `A/V skew=${quality.metrics.avSkewMs?.toFixed(0) ?? 'n/a'}ms (report: ${reportPaths.mdPath})`
  )
  console.log('Linux studio smoke OK - CPU compositor blended three real sources into one recording.')
} finally {
  if (launched) {
    await launched.stop()
  }
  cleanup()
}

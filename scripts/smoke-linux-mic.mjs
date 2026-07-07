// Linux port phase-1 gate: PipeWire microphone capture end to end.
//
// Creates its own virtual microphone (a PulseAudio/PipeWire null sink plus a
// remapped source over its monitor) and plays a tone into it, so the gate
// needs no hardware and no human at the desk. Then drives the real backend
// over its WS protocol and asserts:
//   1. devices.list names the virtual mic as a PipeWire input,
//   2. audio.meter.sample hears the tone (status ready, level above floor),
//   3. a recorded artifact carries the tone on its audio track (analyzer
//      quality gates pass, audio present, no silence dropouts).
//
// Assumptions: Linux with PipeWire + pipewire-pulse (pactl, paplay) and a
// system FFmpeg on PATH — the standard desktop PipeWire stack.

import { execFileSync, spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { launchDevApp } from './lib/app-launcher.mjs'
import { analyzeRecording, writeReports } from './lib/recording-analyzer.mjs'
import { connectBackend, request } from './smoke-recording-session.mjs'

const SINK_NAME = 'videorc_smoke_sink'
const MIC_NAME = 'videorc_smoke_mic'
const MIC_DESCRIPTION = 'VideorcSmokeMic'
const RECORDING_MS = Number(process.env.VIDEORC_SMOKE_RECORDING_MS ?? 3000)
const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 120000)
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

if (process.platform !== 'linux') {
  console.log('Linux mic smoke: skipped (not Linux).')
  process.exit(0)
}

const scratchDir = mkdtempSync(join(tmpdir(), 'videorc-linux-mic-smoke-'))
const outputDirectory = join(scratchDir, 'recordings')
const moduleIds = []
let toneProcess
let launched

function pactl(...args) {
  return execFileSync('pactl', args, { encoding: 'utf8' }).trim()
}

function cleanup() {
  if (toneProcess && toneProcess.exitCode == null) {
    toneProcess.kill('SIGKILL')
  }
  for (const moduleId of moduleIds.reverse()) {
    try {
      pactl('unload-module', moduleId)
    } catch {
      // Already gone (e.g. the daemon restarted) — nothing to unload.
    }
  }
}

try {
  // Virtual mic: tone -> null sink -> monitor -> remapped Audio/Source node.
  moduleIds.push(
    pactl(
      'load-module',
      'module-null-sink',
      `sink_name=${SINK_NAME}`,
      'sink_properties=device.description=VideorcSmokeSink'
    )
  )
  moduleIds.push(
    pactl(
      'load-module',
      'module-remap-source',
      `master=${SINK_NAME}.monitor`,
      `source_name=${MIC_NAME}`,
      `source_properties=device.description=${MIC_DESCRIPTION}`
    )
  )

  const tonePath = join(scratchDir, 'tone.wav')
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
  const connection = launched.connections['backend-ready']
  const ws = await connectBackend(connection, timeoutMs)

  // 1. The virtual mic shows up as a PipeWire input.
  const deviceList = await request(ws, timeoutMs, 'devices.list')
  const microphone = (deviceList.devices ?? []).find(
    (device) => device.kind === 'microphone' && device.name.includes(MIC_DESCRIPTION)
  )
  if (!microphone) {
    const names = (deviceList.devices ?? [])
      .filter((device) => device.kind === 'microphone')
      .map((device) => `${device.name} (${device.id})`)
    throw new Error(`Virtual mic not listed. Microphones seen: ${names.join(', ') || 'none'}`)
  }
  if (!microphone.id.startsWith('microphone:pipewire:')) {
    throw new Error(`Virtual mic has a non-PipeWire id: ${microphone.id}`)
  }
  if (microphone.status !== 'available') {
    throw new Error(`Virtual mic is not available: ${microphone.status}`)
  }
  console.log(`Linux mic smoke devices PASS: ${microphone.name} -> ${microphone.id}`)

  // 2. The audio check meter hears the tone.
  const meter = await request(ws, timeoutMs, 'audio.meter.sample', {
    microphoneId: microphone.id,
    microphoneGainDb: 0,
    microphoneMuted: false
  })
  if (meter.status !== 'ready') {
    throw new Error(
      `Meter did not hear the tone: status=${meter.status} message=${meter.message ?? ''}`
    )
  }
  if (!(meter.level > 0.05)) {
    throw new Error(`Meter level is implausibly low for a tone: ${meter.level}`)
  }
  console.log(
    `Linux mic smoke meter PASS: status=${meter.status} level=${meter.level.toFixed(2)} ` +
      `peakDb=${meter.peakDb?.toFixed(1)}`
  )

  // 3. Desktop audio: the smoke's own null sink is a sink like any other —
  // its monitor must list as a desktop-audio input and the meter must hear
  // the tone through the capture-sink path.
  const monitor = (deviceList.devices ?? []).find(
    (device) => device.kind === 'microphone' && device.name === 'Monitor of VideorcSmokeSink'
  )
  if (!monitor) {
    throw new Error('Sink monitor was not listed as a desktop-audio input.')
  }
  if (!monitor.detail?.startsWith('PipeWire desktop audio')) {
    throw new Error(`Sink monitor has unexpected detail: ${monitor.detail}`)
  }
  const monitorMeter = await request(ws, timeoutMs, 'audio.meter.sample', {
    microphoneId: monitor.id,
    microphoneGainDb: 0,
    microphoneMuted: false
  })
  if (monitorMeter.status !== 'ready' || !(monitorMeter.level > 0.05)) {
    throw new Error(
      `Desktop-audio meter did not hear the tone: status=${monitorMeter.status} ` +
        `level=${monitorMeter.level}`
    )
  }
  console.log(
    `Linux mic smoke desktop-audio PASS: ${monitor.name} -> ${monitor.id} ` +
      `level=${monitorMeter.level.toFixed(2)}`
  )

  // 4. A recording with this mic carries its audio.
  const started = await request(ws, timeoutMs, 'session.start', {
    sources: { testPattern: true, microphoneId: microphone.id },
    layout: {
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
    },
    output: {
      recordEnabled: true,
      streamEnabled: false,
      outputDirectory,
      ffmpegPath: 'ffmpeg',
      video: { preset: 'custom', width: 640, height: 360, fps: 30, bitrateKbps: 2000 },
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
    // The test pattern is static by design; motion stays a warning.
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
    throw new Error('Recording has no analyzable audio track.')
  }
  if (quality.metrics.longestSilenceMs > 500) {
    throw new Error(
      `Mic track went silent for ${quality.metrics.longestSilenceMs.toFixed(0)}ms while a ` +
        `continuous tone was playing (report: ${reportPaths.mdPath})`
    )
  }
  console.log(
    `Linux mic smoke recording PASS: ${outputPath} silence=${quality.metrics.longestSilenceMs.toFixed(0)}ms ` +
      `A/V skew=${quality.metrics.avSkewMs?.toFixed(0)}ms (report: ${reportPaths.mdPath})`
  )
  console.log('Linux mic smoke OK - device listing, meter, and recorded audio verified.')
} finally {
  if (launched) {
    await launched.stop()
  }
  cleanup()
}

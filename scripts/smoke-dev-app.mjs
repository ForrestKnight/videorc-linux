import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawn } from 'node:child_process'

import { smokeAppEnv, stopProcess } from './lib/app-launcher.mjs'
import { runBackendRecordingSmoke } from './smoke-recording-session.mjs'

const repoRoot = resolve(import.meta.dirname, '..')
const outputDirectory = resolve(
  process.env.VIDEORC_SMOKE_OUTPUT_DIR ?? join(tmpdir(), `videorc-dev-smoke-${Date.now()}`)
)
const userDataDir =
  process.env.VIDEORC_USER_DATA_DIR ?? mkdtempSync(join(tmpdir(), 'videorc-dev-smoke-user-data-'))
const ffmpegPath = process.env.VIDEORC_SMOKE_FFMPEG_PATH ?? 'ffmpeg'
const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 90000)

let appProcess
let stopping = false

try {
  const connection = await launchAndReadConnection()
  await runBackendRecordingSmoke({
    connection,
    ffmpegPath,
    outputDirectory,
    timeoutMs,
    label: 'Dev app'
  })
} finally {
  await stopApp()
}

function launchAndReadConnection() {
  return new Promise((resolveConnection, rejectConnection) => {
    const timer = setTimeout(() => {
      rejectConnection(new Error(`Timed out waiting for dev backend READY after ${timeoutMs}ms.`))
    }, timeoutMs)

    appProcess = spawn('pnpm', ['dev'], {
      cwd: repoRoot,
      detached: true,
      env: smokeAppEnv({
        VIDEORC_USER_DATA_DIR: userDataDir,
        VIDEORC_SMOKE_PRINT_BACKEND_READY: '1'
      }),
      stdio: ['ignore', 'pipe', 'pipe']
    })

    appProcess.stdout.setEncoding('utf8')
    appProcess.stderr.setEncoding('utf8')
    appProcess.stdout.on('data', (text) => handleAppOutput(text, resolveConnection, timer))
    appProcess.stderr.on('data', (text) => handleAppOutput(text, resolveConnection, timer))
    appProcess.on('error', (error) => {
      clearTimeout(timer)
      rejectConnection(error)
    })
    appProcess.on('exit', (code, signal) => {
      clearTimeout(timer)
      rejectConnection(
        new Error(`Dev app exited before smoke test completed: code=${code} signal=${signal}`)
      )
    })
  })
}

function handleAppOutput(text, resolveConnection, timer) {
  for (const line of text.split(/\r?\n/)) {
    if (line.trim() && !stopping) {
      console.log(line)
    }

    const marker = '[smoke] backend-ready '
    const index = line.indexOf(marker)
    if (index === -1) {
      continue
    }

    clearTimeout(timer)
    resolveConnection(JSON.parse(line.slice(index + marker.length)))
  }
}

async function stopApp() {
  if (!appProcess?.pid || appProcess.killed) {
    return
  }
  stopping = true
  await stopProcess(appProcess)
}

import { spawn } from 'node:child_process'
import { resolve } from 'node:path'

import { connectBackend, request } from './smoke-recording-session.mjs'

const repoRoot = resolve(import.meta.dirname, '..')
const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 90000)

let appProcess
let stopping = false

try {
  const connection = await launchAndReadConnection()
  const ws = await connectBackend(connection, timeoutMs)
  try {
    const credentials = await request(ws, timeoutMs, 'platformAccounts.oauth.providerCredentials')
    assertProviderCredentials(credentials)

    const capability = await request(ws, timeoutMs, 'streamTargets.x.capability', {})
    assertXCapability(capability)

    const prepare = await requestRaw(ws, timeoutMs, 'streamTargets.x.prepare', {})
    if (prepare.ok || prepare.error?.code !== 'x-native-live-unavailable') {
      throw new Error(`X native prepare should stay unavailable, got ${JSON.stringify(prepare)}`)
    }

    console.log('OAuth guard smoke OK - credential readiness and X native guard verified.')
  } finally {
    ws.close()
  }
} finally {
  await stopApp()
}

function assertProviderCredentials(credentials) {
  if (!Array.isArray(credentials)) {
    throw new Error(`Provider credentials response was not an array: ${JSON.stringify(credentials)}`)
  }
  const byPlatform = new Map(credentials.map((credential) => [credential.platform, credential]))

  const youtube = requireCredential(byPlatform, 'youtube')
  if (!youtube.ready || !youtube.pkce || !youtube.clientIdPresent || youtube.clientSecretPresent) {
    throw new Error(`YouTube PKCE readiness mismatch: ${JSON.stringify(youtube)}`)
  }

  const twitch = requireCredential(byPlatform, 'twitch')
  if (twitch.ready || twitch.pkce || !twitch.clientIdPresent || twitch.clientSecretPresent) {
    throw new Error(`Twitch secret gate mismatch: ${JSON.stringify(twitch)}`)
  }
  if (!String(twitch.message).toLowerCase().includes('client secret')) {
    throw new Error(`Twitch missing-secret message was not explicit: ${JSON.stringify(twitch)}`)
  }

  const x = requireCredential(byPlatform, 'x')
  if (!x.ready || !x.pkce || !x.clientIdPresent || x.clientSecretPresent) {
    throw new Error(`X PKCE readiness mismatch: ${JSON.stringify(x)}`)
  }
}

function requireCredential(byPlatform, platform) {
  const credential = byPlatform.get(platform)
  if (!credential) {
    throw new Error(`Missing credential status for ${platform}.`)
  }
  if (credential.clientIdSource !== 'environment') {
    throw new Error(`${platform} should use the smoke environment client ID, got ${credential.clientIdSource}.`)
  }
  return credential
}

function assertXCapability(capability) {
  if (
    capability?.platform !== 'x' ||
    capability.state !== 'partner-api-required' ||
    capability.nativeAvailable !== false ||
    capability.manualRtmpAvailable !== true ||
    capability.oauthConnected !== false
  ) {
    throw new Error(`X native capability guard mismatch: ${JSON.stringify(capability)}`)
  }
  if (!String(capability.message).includes('partner/API path')) {
    throw new Error(`X capability message should explain the partner/API path: ${JSON.stringify(capability)}`)
  }
  if (!String(capability.docsUrl).startsWith('https://help.x.com/')) {
    throw new Error(`X capability should include Producer docs URL: ${JSON.stringify(capability)}`)
  }
  if (!String(capability.apiOverviewUrl).startsWith('https://docs.x.com/')) {
    throw new Error(`X capability should include X API overview URL: ${JSON.stringify(capability)}`)
  }
}

function requestRaw(ws, timeoutMs, method, params) {
  const id = `smoke-${Date.now()}-${Math.random().toString(16).slice(2)}`
  return new Promise((resolveRequest, rejectRequest) => {
    const timer = setTimeout(() => {
      ws.removeEventListener('message', onMessage)
      rejectRequest(new Error(`Timed out waiting for ${method}.`))
    }, timeoutMs)

    const onMessage = (event) => {
      let message
      try {
        message = JSON.parse(event.data)
      } catch (error) {
        clearTimeout(timer)
        ws.removeEventListener('message', onMessage)
        rejectRequest(error)
        return
      }
      if (message.id !== id) {
        return
      }

      clearTimeout(timer)
      ws.removeEventListener('message', onMessage)
      resolveRequest(message)
    }

    ws.addEventListener('message', onMessage)
    ws.send(JSON.stringify({ id, method, params }))
  })
}

function launchAndReadConnection() {
  return new Promise((resolveConnection, rejectConnection) => {
    const timer = setTimeout(() => {
      rejectConnection(new Error(`Timed out waiting for dev backend READY after ${timeoutMs}ms.`))
    }, timeoutMs)

    appProcess = spawn('pnpm', ['dev'], {
      cwd: repoRoot,
      detached: true,
      env: {
        ...process.env,
        VIDEORC_SMOKE_PRINT_BACKEND_READY: '1',
        VIDEORC_YOUTUBE_CLIENT_ID: 'smoke-youtube-client-id',
        VIDEORC_TWITCH_CLIENT_ID: 'smoke-twitch-client-id',
        VIDEORC_X_CLIENT_ID: 'smoke-x-client-id',
        VIDEORC_TWITCH_CLIENT_SECRET: ''
      },
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
      rejectConnection(new Error(`Dev app exited before OAuth guard smoke completed: code=${code} signal=${signal}`))
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

function stopApp() {
  return new Promise((resolveStop) => {
    if (!appProcess?.pid || appProcess.killed) {
      resolveStop()
      return
    }

    const timer = setTimeout(() => {
      killApp('SIGKILL')
      resolveStop()
    }, 5000)

    stopping = true
    appProcess.once('exit', () => {
      clearTimeout(timer)
      resolveStop()
    })
    killApp('SIGTERM')
  })
}

function killApp(signal) {
  if (!appProcess?.pid) {
    return
  }

  try {
    process.kill(-appProcess.pid, signal)
  } catch {
    appProcess.kill(signal)
  }
}

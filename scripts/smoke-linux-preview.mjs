// Linux port: the software preview bridge feeds the composited scene to the
// JPEG poller the Studio panel renders inline.
//
// On Linux there is no Metal surface to present, so the compositor's output is
// encoded to JPEG in-process and served at /preview/live.jpg. This drives the
// real backend: create a preview surface (which starts the compositor), then
// poll /preview/live.jpg and assert a valid, non-trivial JPEG of the scene
// comes back. No hardware or portal grant needed — the synthetic compositor
// runs at idle.

import { request as httpRequest } from 'node:http'

import { launchDevApp } from './lib/app-launcher.mjs'
import { connectBackend, request } from './smoke-recording-session.mjs'

const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 120000)
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

if (process.platform !== 'linux') {
  console.log('Linux preview smoke: skipped (not Linux).')
  process.exit(0)
}

function fetchBinary(connection, path) {
  return new Promise((resolve, reject) => {
    const req = httpRequest(
      {
        host: connection.host,
        port: connection.port,
        path: `${path}?token=${encodeURIComponent(connection.token)}`,
        method: 'GET'
      },
      (res) => {
        const chunks = []
        res.on('data', (chunk) => chunks.push(chunk))
        res.on('end', () => resolve({ status: res.statusCode, body: Buffer.concat(chunks) }))
      }
    )
    req.on('error', reject)
    req.end()
  })
}

let launched
try {
  launched = await launchDevApp({
    requiredMarkers: ['backend-ready'],
    timeoutMs,
    env: { VIDEORC_SMOKE_PRINT_BACKEND_READY: '1' }
  })
  const connection = launched.connections['backend-ready']
  const ws = await connectBackend(connection, timeoutMs)

  // Start the compositor by creating a preview surface (idle preview path).
  await request(ws, timeoutMs, 'preview.surface.create', {
    bounds: {
      screenX: 100,
      screenY: 100,
      width: 1280,
      height: 720,
      scaleFactor: 1
    },
    targetFps: 30
  })

  // Give the compositor + JPEG bridge a moment to produce frames.
  let frame = null
  for (let attempt = 0; attempt < 40; attempt++) {
    await sleep(250)
    const res = await fetchBinary(connection, '/preview/live.jpg')
    if (res.status === 200 && res.body.length > 0) {
      frame = res.body
      break
    }
  }
  if (!frame) {
    throw new Error('The preview JPEG endpoint never served a frame (compositor bridge idle?).')
  }
  // JPEG SOI marker.
  if (frame[0] !== 0xff || frame[1] !== 0xd8) {
    throw new Error(`Preview endpoint returned non-JPEG bytes (${frame.length} bytes).`)
  }
  if (frame.length < 500) {
    throw new Error(`Preview JPEG is implausibly small (${frame.length} bytes) — likely blank.`)
  }
  console.log(`Linux preview smoke frame PASS: ${frame.length}-byte composited JPEG served`)

  // A second poll a moment later should also succeed (the bridge keeps running).
  await sleep(500)
  const again = await fetchBinary(connection, '/preview/live.jpg')
  if (again.status !== 200 || again.body.length < 500) {
    throw new Error('Preview JPEG stopped being served after the first frame.')
  }
  console.log('Linux preview smoke OK - compositor output is served as JPEG for the inline panel.')
} finally {
  if (launched) {
    await launched.stop()
  }
}

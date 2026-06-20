import assert from 'node:assert/strict'
import { mkdtempSync } from 'node:fs'
import { rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { launchDevApp, repoRoot } from './lib/app-launcher.mjs'
import {
  collectProcessCensus,
  formatCensus,
  ownedProcessLedgerPaths,
  pruneDeadOwnedProcessRecords,
  waitForCleanProcessState,
  waitForNoLiveProcessState
} from './lib/process-census.mjs'
import {
  evaluateProcessMemoryGate,
  formatProcessMemorySummary,
  summarizeProcessMemory
} from './lib/process-memory-gate.mjs'

const timeoutMs = Number(process.env.VIDEORC_SMOKE_TIMEOUT_MS ?? 120000)
const sampleMs = Number(process.env.VIDEORC_PROCESS_MEMORY_SAMPLE_MS ?? 5000)
const intervalMs = Number(process.env.VIDEORC_PROCESS_MEMORY_INTERVAL_MS ?? 1000)
const thresholds = {
  maxTotalRssMb: Number(process.env.VIDEORC_PROCESS_MEMORY_MAX_TOTAL_MB ?? 4096),
  maxOwnedRssMb: Number(process.env.VIDEORC_PROCESS_MEMORY_MAX_OWNED_MB ?? 1024),
  maxRoleRssMb: {
    backend: Number(process.env.VIDEORC_PROCESS_MEMORY_MAX_BACKEND_MB ?? 512),
    'native-preview-helper': Number(process.env.VIDEORC_PROCESS_MEMORY_MAX_HELPER_MB ?? 512)
  }
}

const stateRoot = mkdtempSync(join(tmpdir(), 'videorc-process-memory-'))
const appDataDir = join(stateRoot, 'app-data')
const userDataDir = join(stateRoot, 'user-data')
const ledgerPaths = ownedProcessLedgerPaths({
  appDataDir,
  userDataDir,
  workspaceRoot: repoRoot
})

let launched

try {
  launched = await launchDevApp({
    env: {
      VIDEORC_APP_DATA_DIR: appDataDir,
      VIDEORC_USER_DATA_DIR: userDataDir,
      VIDEORC_DISABLE_AUTO_PREVIEW: '1'
    },
    timeoutMs,
    requiredMarkers: ['backend-ready'],
    onLine: (line) => {
      if (/Reaping|Backend exited|Native preview host helper|error|panic/i.test(line)) {
        console.log(line)
      }
    }
  })

  const samples = await collectSamples()
  const summary = summarizeProcessMemory(samples)
  const failures = evaluateProcessMemoryGate(summary, thresholds)

  console.log('\n=== process memory summary ===')
  console.log(formatProcessMemorySummary(summary))

  assert.equal(failures.length, 0, `Process memory gate failed:\n${failures.join('\n')}`)
} finally {
  if (launched) {
    await launched.stop()
  }

  const stopped = await waitForNoLiveProcessState({
    ledgerPaths,
    pgid: launched?.process?.pid,
    timeoutMs: 10000
  })
  const pruned = await pruneDeadOwnedProcessRecords({ ledgerPaths })
  for (const entry of pruned) {
    console.log(
      `pruned ${entry.removed.length} dead owned process record(s) from ${entry.ledgerPath}`
    )
  }

  const clean = await waitForCleanProcessState({
    ledgerPaths,
    pgid: launched?.process?.pid,
    timeoutMs: 1000
  })
  if (process.env.VIDEORC_PROCESS_MEMORY_PRINT_TEARDOWN === '1') {
    console.log('\n=== teardown process census ===')
    console.log(formatCensus(stopped))
    console.log('\n=== clean process census ===')
    console.log(formatCensus(clean))
  }

  await rm(stateRoot, { recursive: true, force: true })
}

console.log('Process memory smoke OK - process tree RSS stayed inside configured bounds.')

async function collectSamples() {
  const startedAt = Date.now()
  const samples = []
  for (;;) {
    samples.push(
      await collectProcessCensus({
        ledgerPaths,
        pgid: launched.process.pid
      })
    )
    const remainingMs = sampleMs - (Date.now() - startedAt)
    if (remainingMs <= 0) {
      break
    }
    await sleep(Math.min(intervalMs, remainingMs))
  }
  return samples
}

function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms))
}

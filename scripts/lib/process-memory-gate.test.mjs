import assert from 'node:assert/strict'
import test from 'node:test'

import {
  evaluateProcessMemoryGate,
  formatProcessMemorySummary,
  summarizeProcessMemory
} from './process-memory-gate.mjs'

test('summarizeProcessMemory tracks peak process tree, owned RSS, and role totals', () => {
  const summary = summarizeProcessMemory([
    census({
      alive: [10],
      rows: [row(10, 'backend', 20_000), row(11, 'electron-main', 100_000)]
    }),
    census({
      alive: [10, 12],
      rows: [
        row(10, 'backend', 30_000),
        row(12, 'native-preview-helper', 40_000),
        row(13, 'electron-main', 120_000)
      ]
    })
  ])

  assert.equal(summary.samples, 2)
  assert.equal(summary.maxTotalRssKb, 190_000)
  assert.equal(summary.maxOwnedRssKb, 70_000)
  assert.deepEqual(summary.roles.backend, { maxCount: 1, maxRssKb: 30_000 })
  assert.deepEqual(summary.roles['native-preview-helper'], { maxCount: 1, maxRssKb: 40_000 })
})

test('evaluateProcessMemoryGate reports breached total, owned, and role thresholds', () => {
  const failures = evaluateProcessMemoryGate(
    {
      maxTotalRssKb: 5 * 1024,
      maxOwnedRssKb: 3 * 1024,
      roles: {
        backend: { maxCount: 1, maxRssKb: 2 * 1024 }
      }
    },
    {
      maxTotalRssMb: 4,
      maxOwnedRssMb: 2,
      maxRoleRssMb: { backend: 1 }
    }
  )

  assert.deepEqual(failures, [
    'total process tree RSS 5MB exceeded 4MB',
    'owned process RSS 3MB exceeded 2MB',
    'backend RSS 2MB exceeded 1MB'
  ])
})

test('formatProcessMemorySummary emits a stable role report', () => {
  assert.equal(
    formatProcessMemorySummary({
      samples: 1,
      maxTotalRssKb: 2048,
      maxOwnedRssKb: 1024,
      roles: {
        backend: { maxCount: 1, maxRssKb: 1024 },
        tooling: { maxCount: 2, maxRssKb: 2048 }
      }
    }),
    [
      'samples: 1',
      'max total process tree RSS: 2MB',
      'max owned process RSS: 1MB',
      '  backend: max_count=1 max_rss=1MB',
      '  tooling: max_count=2 max_rss=2MB'
    ].join('\n')
  )
})

function census({ alive, rows }) {
  return {
    aliveRecords: alive.map((pid) => ({ pid })),
    processRows: rows
  }
}

function row(pid, role, rssKb) {
  return { pid, role, rssKb }
}

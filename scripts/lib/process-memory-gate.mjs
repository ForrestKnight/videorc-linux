export function summarizeProcessMemory(censuses) {
  const summary = {
    samples: censuses.length,
    maxTotalRssKb: 0,
    maxOwnedRssKb: 0,
    roles: {}
  }

  for (const census of censuses) {
    const ownedPids = new Set(census.aliveRecords.map((record) => record.pid))
    const roleTotals = {}
    let totalRssKb = 0
    let ownedRssKb = 0

    for (const row of census.processRows) {
      const rssKb = finiteNumber(row.rssKb)
      totalRssKb += rssKb
      if (ownedPids.has(row.pid)) {
        ownedRssKb += rssKb
      }

      const role = row.role ?? 'other'
      const roleTotal = (roleTotals[role] ??= { count: 0, rssKb: 0 })
      roleTotal.count += 1
      roleTotal.rssKb += rssKb
    }

    summary.maxTotalRssKb = Math.max(summary.maxTotalRssKb, totalRssKb)
    summary.maxOwnedRssKb = Math.max(summary.maxOwnedRssKb, ownedRssKb)
    for (const [role, total] of Object.entries(roleTotals)) {
      const entry = (summary.roles[role] ??= { maxCount: 0, maxRssKb: 0 })
      entry.maxCount = Math.max(entry.maxCount, total.count)
      entry.maxRssKb = Math.max(entry.maxRssKb, total.rssKb)
    }
  }

  return summary
}

export function evaluateProcessMemoryGate(summary, thresholds = {}) {
  const failures = []
  addLimitFailure(
    failures,
    'total process tree RSS',
    summary.maxTotalRssKb,
    thresholds.maxTotalRssMb
  )
  addLimitFailure(failures, 'owned process RSS', summary.maxOwnedRssKb, thresholds.maxOwnedRssMb)

  for (const [role, maxRssMb] of Object.entries(thresholds.maxRoleRssMb ?? {})) {
    addLimitFailure(failures, `${role} RSS`, summary.roles[role]?.maxRssKb ?? 0, maxRssMb)
  }

  return failures
}

export function formatProcessMemorySummary(summary) {
  const lines = [
    `samples: ${summary.samples}`,
    `max total process tree RSS: ${formatMb(summary.maxTotalRssKb)}`,
    `max owned process RSS: ${formatMb(summary.maxOwnedRssKb)}`
  ]
  for (const [role, totals] of Object.entries(summary.roles).sort(([a], [b]) =>
    a.localeCompare(b)
  )) {
    lines.push(`  ${role}: max_count=${totals.maxCount} max_rss=${formatMb(totals.maxRssKb)}`)
  }
  return lines.join('\n')
}

function addLimitFailure(failures, label, actualKb, limitMb) {
  if (!Number.isFinite(limitMb) || limitMb <= 0) {
    return
  }
  const limitKb = limitMb * 1024
  if (actualKb > limitKb) {
    failures.push(`${label} ${formatMb(actualKb)} exceeded ${limitMb}MB`)
  }
}

function formatMb(kb) {
  return `${Math.round(kb / 1024)}MB`
}

function finiteNumber(value) {
  return Number.isFinite(value) ? value : 0
}

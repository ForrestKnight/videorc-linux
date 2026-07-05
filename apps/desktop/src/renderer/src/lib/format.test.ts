import { describe, expect, it } from 'vitest'

import { durationMsLabel, formatBytes } from './format'

describe('format', () => {
  it('spells out recording durations once they pass an hour', () => {
    expect(durationMsLabel(72 * 60 * 1000)).toBe('1 hour and 12 minutes')
    expect(durationMsLabel(60 * 60 * 1000)).toBe('1 hour')
    expect(durationMsLabel(121 * 60 * 1000)).toBe('2 hours and 1 minute')
  })

  it('keeps sub-hour recording durations compact', () => {
    expect(durationMsLabel(12 * 60 * 1000 + 34 * 1000)).toBe('12:34')
    expect(durationMsLabel(undefined)).toBe('--:--')
  })
})

describe('formatBytes', () => {
  it('scales through the units with sensible precision', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(742 * 1024 * 1024)).toBe('742 MB')
    expect(formatBytes(1.2 * 1024 ** 3)).toBe('1.2 GB')
    expect(formatBytes(175 * 1024 ** 3)).toBe('175 GB')
  })

  it('answers a calm dash for the unknowable', () => {
    expect(formatBytes(undefined)).toBe('—')
    expect(formatBytes(null)).toBe('—')
    expect(formatBytes(-5)).toBe('—')
    expect(formatBytes(Number.NaN)).toBe('—')
  })
})

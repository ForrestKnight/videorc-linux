import { describe, expect, it } from 'vitest'

import {
  diffActivity,
  recordingTransitionLabel,
  relativeAgo,
  type ActivitySnapshot
} from './studio-activity'

const base: ActivitySnapshot = {
  recordingState: 'idle',
  deviceIds: ['mic:1', 'cam:1'],
  videoPreset: '1080p',
  layoutPreset: 'screen-camera',
  microphoneMuted: false,
  screenSourceKey: 'screen:1',
  cameraId: 'cam:1',
  microphoneId: 'mic:1'
}

describe('recordingTransitionLabel', () => {
  it('labels meaningful states and ignores transients', () => {
    expect(recordingTransitionLabel('recording')).toBe('Recording started')
    expect(recordingTransitionLabel('streaming')).toBe('Streaming started')
    expect(recordingTransitionLabel('idle')).toBe('Session ended')
    expect(recordingTransitionLabel('failed')).toBe('Session failed')
    expect(recordingTransitionLabel('starting')).toBeNull()
    expect(recordingTransitionLabel('stopping')).toBeNull()
  })
})

describe('diffActivity', () => {
  it('returns nothing when nothing changed', () => {
    expect(diffActivity(base, base)).toEqual([])
  })

  it('reports a recording start', () => {
    expect(diffActivity(base, { ...base, recordingState: 'recording' })).toEqual([
      'Recording started'
    ])
  })

  it('counts connected and disconnected devices', () => {
    const next = { ...base, deviceIds: ['cam:1', 'mic:2', 'mic:3'] }
    expect(diffActivity(base, next)).toEqual(['2 devices connected', 'Device disconnected'])
  })

  it('reports mute, layout, output, and source changes', () => {
    expect(diffActivity(base, { ...base, microphoneMuted: true })).toEqual(['Microphone muted'])
    expect(diffActivity(base, { ...base, layoutPreset: 'screen-only' })).toEqual(['Layout changed'])
    expect(diffActivity(base, { ...base, videoPreset: '4k' })).toEqual(['Output preset changed'])
    expect(diffActivity(base, { ...base, cameraId: 'cam:2' })).toEqual(['Camera changed'])
    expect(diffActivity(base, { ...base, screenSourceKey: 'window:9' })).toEqual([
      'Screen source changed'
    ])
  })
})

describe('relativeAgo', () => {
  it('formats across units', () => {
    expect(relativeAgo(2_000)).toBe('just now')
    expect(relativeAgo(12_000)).toBe('12s ago')
    expect(relativeAgo(3 * 60_000)).toBe('3m ago')
    expect(relativeAgo(2 * 3_600_000)).toBe('2h ago')
    expect(relativeAgo(3 * 86_400_000)).toBe('3d ago')
  })
})

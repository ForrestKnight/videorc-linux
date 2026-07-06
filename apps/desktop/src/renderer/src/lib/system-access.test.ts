import { describe, expect, it } from 'vitest'

import type { AudioMeterResult, DeviceList } from '@/lib/backend'

import {
  cameraAccessState,
  microphoneAccessState,
  screenAccessState,
  shouldShowPermissionsOnboarding,
  systemAccessRows
} from './system-access'

function devices(list: Partial<DeviceList['devices'][number]>[]): DeviceList {
  return {
    devices: list.map((device, index) => ({
      id: device.id ?? `device-${index}`,
      name: device.name ?? `Device ${index}`,
      kind: device.kind ?? 'camera',
      status: device.status ?? 'available'
    })),
    warnings: []
  }
}

describe('screenAccessState', () => {
  it('reports not-granted when enumeration hits the permission placeholder', () => {
    expect(
      screenAccessState(
        devices([
          { id: 'screen:screencapturekit-missing', kind: 'screen', status: 'permission-required' }
        ])
      )
    ).toBe('not-granted')
  })
  it('reports granted for an available ScreenCaptureKit display', () => {
    expect(
      screenAccessState(
        devices([{ id: 'screen:screencapturekit:1', kind: 'screen', status: 'available' }])
      )
    ).toBe('granted')
  })
  it('stays honest (first-use) with no signal', () => {
    expect(screenAccessState(devices([]))).toBe('first-use')
  })
})

describe('cameraAccessState', () => {
  it('not-granted beats available (any blocked camera means the grant is missing)', () => {
    expect(
      cameraAccessState(
        devices([
          { kind: 'camera', status: 'permission-required' },
          { kind: 'camera', status: 'available' }
        ])
      )
    ).toBe('not-granted')
  })
  it('granted when cameras enumerate as available', () => {
    expect(cameraAccessState(devices([{ kind: 'camera', status: 'available' }]))).toBe('granted')
  })
})

describe('microphoneAccessState', () => {
  const meter = (status: AudioMeterResult['status']): AudioMeterResult => ({ status })
  it('maps probe results to states, never guessing green', () => {
    expect(microphoneAccessState(null)).toBe('first-use')
    expect(microphoneAccessState(meter('permission-required'))).toBe('not-granted')
    expect(microphoneAccessState(meter('no-frames'))).toBe('device-issue')
    expect(microphoneAccessState(meter('ready'))).toBe('granted')
    expect(microphoneAccessState(meter('silent'))).toBe('granted')
  })
})

describe('shouldShowPermissionsOnboarding', () => {
  const allGranted = systemAccessRows({
    deviceList: devices([
      { id: 'screen:screencapturekit:1', kind: 'screen', status: 'available' },
      { kind: 'camera', status: 'available' }
    ]),
    audioMeter: { status: 'ready' }
  })
  const missingSome = systemAccessRows({
    deviceList: devices([
      { id: 'screen:screencapturekit-missing', kind: 'screen', status: 'permission-required' }
    ]),
    audioMeter: null
  })

  it('shows for a fresh machine with missing grants', () => {
    expect(
      shouldShowPermissionsOnboarding({ rows: missingSome, dismissed: false, backendReady: true })
    ).toBe(true)
  })
  it('never shows when every grant already exists (reinstalls skip onboarding)', () => {
    expect(
      shouldShowPermissionsOnboarding({ rows: allGranted, dismissed: false, backendReady: true })
    ).toBe(false)
  })
  it('first-use counts as missing — that is what drives the native prompts', () => {
    const firstUse = systemAccessRows({ deviceList: devices([]), audioMeter: null })
    expect(
      shouldShowPermissionsOnboarding({ rows: firstUse, dismissed: false, backendReady: true })
    ).toBe(true)
  })
  it('does not show for a mic device issue because permissions are not the fix', () => {
    const noFrames = systemAccessRows({
      deviceList: devices([
        { id: 'screen:screencapturekit:1', kind: 'screen', status: 'available' },
        { kind: 'camera', status: 'available' }
      ]),
      audioMeter: {
        status: 'no-frames',
        message: 'This microphone opened but did not send audio frames.'
      }
    })

    expect(
      shouldShowPermissionsOnboarding({ rows: noFrames, dismissed: false, backendReady: true })
    ).toBe(false)
    expect(noFrames[2].state).toBe('device-issue')
    expect(noFrames[2].detail).toMatch(/did not send audio frames/)
  })
  it('a dismissal snoozes it even with grants still missing', () => {
    expect(
      shouldShowPermissionsOnboarding({ rows: missingSome, dismissed: true, backendReady: true })
    ).toBe(false)
  })
  it('never shows before the backend has reported real device state', () => {
    expect(
      shouldShowPermissionsOnboarding({ rows: missingSome, dismissed: false, backendReady: false })
    ).toBe(false)
  })
})

describe('systemAccessRows', () => {
  it('renders three rows with purposes and honest details', () => {
    const rows = systemAccessRows({
      deviceList: devices([
        { id: 'screen:screencapturekit:1', kind: 'screen', status: 'available' },
        { kind: 'camera', status: 'permission-required' }
      ]),
      audioMeter: null
    })
    expect(rows.map((row) => row.id)).toEqual(['screen-recording', 'camera', 'microphone'])
    expect(rows[0].state).toBe('granted')
    expect(rows[1].state).toBe('not-granted')
    expect(rows[1].detail).toMatch(/System Settings/)
    expect(rows[2].state).toBe('first-use')
    expect(rows[2].detail).toMatch(/mic check/)
  })
})

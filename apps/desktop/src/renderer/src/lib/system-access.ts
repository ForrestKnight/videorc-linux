import type { AudioMeterResult, Device, DeviceList } from '@/lib/backend'

// ST3 (Settings rework): live permission states for the System access rows.
// Derived from the SAME signals the rest of the app already trusts (device
// enumeration statuses + the audio meter probe) — never from guesses. When
// macOS genuinely won't tell us until first use, we say so instead of faking
// a green chip.

export type SystemAccessState = 'granted' | 'not-granted' | 'first-use' | 'device-issue'

export interface SystemAccessRow {
  id: 'screen-recording' | 'camera' | 'microphone'
  label: string
  /** One line of why this permission matters. */
  purpose: string
  state: SystemAccessState
  detail: string
}

function screenDevices(devices: Device[]): Device[] {
  return devices.filter((device) => device.kind === 'screen' || device.kind === 'window')
}

export function screenAccessState(deviceList: DeviceList): SystemAccessState {
  const devices = screenDevices(deviceList.devices)
  if (devices.some((device) => device.status === 'permission-required')) {
    return 'not-granted'
  }
  if (
    devices.some(
      (device) => device.status === 'available' && device.id.includes('screencapturekit')
    )
  ) {
    return 'granted'
  }
  return 'first-use'
}

export function cameraAccessState(deviceList: DeviceList): SystemAccessState {
  const cameras = deviceList.devices.filter((device) => device.kind === 'camera')
  if (cameras.some((device) => device.status === 'permission-required')) {
    return 'not-granted'
  }
  if (cameras.some((device) => device.status === 'available')) {
    // Enumeration succeeding proves listing works; capture itself is only
    // proven once frames flow, but macOS reports denied cameras as
    // permission-required — available means the grant exists.
    return 'granted'
  }
  return 'first-use'
}

export function microphoneAccessState(audioMeter: AudioMeterResult | null): SystemAccessState {
  if (!audioMeter) {
    return 'first-use'
  }
  if (audioMeter.status === 'permission-required') {
    return 'not-granted'
  }
  if (audioMeter.status === 'no-frames') {
    return 'device-issue'
  }
  if (audioMeter.status === 'ready' || audioMeter.status === 'silent') {
    return 'granted'
  }
  return 'first-use'
}

export function systemAccessRows({
  deviceList,
  audioMeter
}: {
  deviceList: DeviceList
  audioMeter: AudioMeterResult | null
}): SystemAccessRow[] {
  const screen = screenAccessState(deviceList)
  const camera = cameraAccessState(deviceList)
  const microphone = microphoneAccessState(audioMeter)

  return [
    {
      id: 'screen-recording',
      label: 'Screen Recording',
      purpose: 'Capture displays and app windows.',
      state: screen,
      detail: accessDetail(screen, 'screen capture')
    },
    {
      id: 'camera',
      label: 'Camera',
      purpose: 'Camera overlay in your scenes.',
      state: camera,
      detail: accessDetail(camera, 'the camera')
    },
    {
      id: 'microphone',
      label: 'Microphone',
      purpose: 'Voice audio and live captions.',
      state: microphone,
      detail:
        microphone === 'device-issue'
          ? (audioMeter?.message ??
            'The microphone opened but did not send frames. Try the fallback input or another mic.')
          : microphone === 'first-use'
            ? 'Checked when you run a mic check or start a session.'
            : accessDetail(microphone, 'the microphone')
    }
  ]
}

// Permissions onboarding gate: the dialog exists ONLY to collect grants, so it
// shows iff a grant is missing — never for users whose Mac already has them
// (TCC grants persist per bundle id, so reinstalls skip it too). `dismissed`
// is a snooze, not the trigger: once the user continues or skips, gaps go back
// to the Sources alerts and Settings chips. `backendReady` guards the boot
// window where device enumeration hasn't arrived and every state would read
// first-use — unknown must never flash the dialog.
export function shouldShowPermissionsOnboarding({
  rows,
  dismissed,
  backendReady
}: {
  rows: SystemAccessRow[]
  dismissed: boolean
  backendReady: boolean
}): boolean {
  if (dismissed || !backendReady) {
    return false
  }
  return rows.some((row) => row.state !== 'granted' && row.state !== 'device-issue')
}

function accessDetail(state: SystemAccessState, subject: string): string {
  switch (state) {
    case 'granted':
      return 'Permission granted.'
    case 'not-granted':
      // The packaged-app nuance from the 0.9.1 incident: a missing grant is
      // fixed in System Settings; a missing ENTITLEMENT can't be — but 0.9.2+
      // ships the entitlements, so System Settings is the right pointer.
      return `macOS is blocking ${subject} — grant access in System Settings.`
    case 'first-use':
      return 'macOS reports this on first use.'
    case 'device-issue':
      return 'The device opened but did not send audio frames.'
  }
}

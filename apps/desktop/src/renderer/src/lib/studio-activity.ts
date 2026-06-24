// Pure activity-log derivations for the Studio dashboard (SD4). The dashboard
// has no backend event stream for friendly config/device events, so it derives
// a lightweight log from observable state transitions between renders. Kept free
// of React + Date so it runs under the node-only vitest runner.

export type ActivitySnapshot = {
  recordingState: string
  deviceIds: string[]
  videoPreset: string
  layoutPreset: string
  microphoneMuted: boolean
  screenSourceKey: string
  cameraId: string
  microphoneId: string
}

/** The human label for a recording-state transition, or null for transients. */
export function recordingTransitionLabel(state: string): string | null {
  switch (state) {
    case 'recording':
      return 'Recording started'
    case 'streaming':
      return 'Streaming started'
    case 'idle':
      return 'Session ended'
    case 'failed':
      return 'Session failed'
    default:
      return null // starting / stopping are transient — not worth a log line
  }
}

/** The new activity labels implied by moving from `prev` to `next`. */
export function diffActivity(prev: ActivitySnapshot, next: ActivitySnapshot): string[] {
  const events: string[] = []

  if (prev.recordingState !== next.recordingState) {
    const label = recordingTransitionLabel(next.recordingState)
    if (label) {
      events.push(label)
    }
  }

  const added = next.deviceIds.filter((id) => !prev.deviceIds.includes(id)).length
  const removed = prev.deviceIds.filter((id) => !next.deviceIds.includes(id)).length
  if (added > 0) {
    events.push(added === 1 ? 'Device connected' : `${added} devices connected`)
  }
  if (removed > 0) {
    events.push(removed === 1 ? 'Device disconnected' : `${removed} devices disconnected`)
  }

  if (prev.videoPreset !== next.videoPreset) {
    events.push('Output preset changed')
  }
  if (prev.layoutPreset !== next.layoutPreset) {
    events.push('Layout changed')
  }
  if (prev.microphoneMuted !== next.microphoneMuted) {
    events.push(next.microphoneMuted ? 'Microphone muted' : 'Microphone unmuted')
  }
  if (prev.microphoneId !== next.microphoneId) {
    events.push('Microphone changed')
  }
  if (prev.screenSourceKey !== next.screenSourceKey) {
    events.push('Screen source changed')
  }
  if (prev.cameraId !== next.cameraId) {
    events.push('Camera changed')
  }

  return events
}

/** Compact "x ago" for an elapsed millisecond delta. */
export function relativeAgo(deltaMs: number): string {
  if (deltaMs < 5_000) {
    return 'just now'
  }
  const seconds = Math.floor(deltaMs / 1000)
  if (seconds < 60) {
    return `${seconds}s ago`
  }
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) {
    return `${minutes}m ago`
  }
  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    return `${hours}h ago`
  }
  return `${Math.floor(hours / 24)}d ago`
}

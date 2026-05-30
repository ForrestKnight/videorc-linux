export interface BackendConnection {
  host: string
  port: number
  token: string
}

export interface BackendHealth {
  status: string
  version: string
  platform: string
  ffmpeg: ToolStatus
}

export interface ToolStatus {
  path: string
  available: boolean
  version?: string
  message?: string
}

export type DeviceKind = 'screen' | 'window' | 'camera' | 'microphone' | 'system-audio'
export type DeviceStatus = 'available' | 'unavailable' | 'permission-required'

export interface Device {
  id: string
  name: string
  kind: DeviceKind
  status: DeviceStatus
  detail?: string
}

export interface DeviceList {
  devices: Device[]
  warnings: string[]
}

export type RecordingState = 'idle' | 'starting' | 'recording' | 'stopping' | 'failed'

export interface RecordingStatus {
  state: RecordingState
  outputPath?: string
  startedAt?: string
  message?: string
}

export interface BackendLogEvent {
  level: 'info' | 'warn' | 'error' | string
  message: string
  timestamp: string
}

export interface ClientCommand<TParams = unknown> {
  id: string
  method: string
  params?: TParams
}

export interface ServerResponse<TPayload = unknown> {
  id: string
  ok: boolean
  payload?: TPayload
  error?: {
    code: string
    message: string
  }
}

export interface ServerEvent<TPayload = unknown> {
  event: string
  payload: TPayload
}

export interface StartRecordingParams {
  outputDirectory?: string
  ffmpegPath?: string
}

export interface VideogreApi {
  getBackendConnection: () => Promise<BackendConnection | null>
  getBackendLogs: () => Promise<BackendLogEvent[]>
  onBackendConnection: (callback: (connection: BackendConnection) => void) => () => void
  onBackendLog: (callback: (log: BackendLogEvent) => void) => () => void
}

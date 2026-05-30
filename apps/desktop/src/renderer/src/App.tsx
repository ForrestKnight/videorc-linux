import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  CircleStop,
  Folder,
  Mic,
  Monitor,
  Play,
  Radio,
  RefreshCcw,
  Settings,
  Video,
  Volume2
} from 'lucide-react'
import type { ReactElement, ReactNode } from 'react'
import { useCallback, useEffect, useMemo, useState } from 'react'

import type {
  BackendConnection,
  BackendHealth,
  BackendLogEvent,
  Device,
  DeviceKind,
  DeviceList,
  RecordingStatus,
  StartRecordingParams
} from '../../shared/backend'
import { BackendClient } from './backendClient'

type SettingsState = {
  outputDirectory: string
  ffmpegPath: string
}

type WsStatus = 'waiting' | 'connecting' | 'connected' | 'failed' | 'closed'

const defaultSettings: SettingsState = {
  outputDirectory: '',
  ffmpegPath: ''
}

const deviceIcons: Record<DeviceKind, typeof Monitor> = {
  screen: Monitor,
  window: Monitor,
  camera: Video,
  microphone: Mic,
  'system-audio': Volume2
}

function loadSettings(): SettingsState {
  const raw = localStorage.getItem('videogre.settings')
  if (!raw) {
    return defaultSettings
  }

  try {
    return { ...defaultSettings, ...(JSON.parse(raw) as Partial<SettingsState>) }
  } catch {
    return defaultSettings
  }
}

function compactTime(timestamp: string): string {
  try {
    return new Intl.DateTimeFormat(undefined, {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    }).format(new Date(timestamp))
  } catch {
    return timestamp
  }
}

export function App(): ReactElement {
  const [connection, setConnection] = useState<BackendConnection | null>(null)
  const [client, setClient] = useState<BackendClient | null>(null)
  const [wsStatus, setWsStatus] = useState<WsStatus>('waiting')
  const [health, setHealth] = useState<BackendHealth | null>(null)
  const [deviceList, setDeviceList] = useState<DeviceList>({ devices: [], warnings: [] })
  const [recording, setRecording] = useState<RecordingStatus>({ state: 'idle', message: 'Ready.' })
  const [logs, setLogs] = useState<BackendLogEvent[]>([])
  const [settings, setSettings] = useState<SettingsState>(() => loadSettings())
  const [lastError, setLastError] = useState<string | null>(null)

  const requestParams = useMemo<StartRecordingParams>(
    () => ({
      outputDirectory: settings.outputDirectory.trim() || undefined,
      ffmpegPath: settings.ffmpegPath.trim() || undefined
    }),
    [settings]
  )

  const appendLog = useCallback((log: BackendLogEvent) => {
    setLogs((current) => [...current.slice(-79), log])
  }, [])

  useEffect(() => {
    localStorage.setItem('videogre.settings', JSON.stringify(settings))
  }, [settings])

  useEffect(() => {
    let disposed = false

    window.videogre.getBackendLogs().then((backendLogs) => {
      if (!disposed) {
        setLogs(backendLogs.slice(-80))
      }
    })
    window.videogre.getBackendConnection().then((nextConnection) => {
      if (!disposed && nextConnection) {
        setConnection(nextConnection)
      }
    })

    const offConnection = window.videogre.onBackendConnection(setConnection)
    const offLog = window.videogre.onBackendLog(appendLog)

    return () => {
      disposed = true
      offConnection()
      offLog()
    }
  }, [appendLog])

  useEffect(() => {
    if (!connection) {
      return
    }

    const nextClient = new BackendClient(connection)
    setClient(nextClient)
    setWsStatus('connecting')
    setLastError(null)

    const unsubscribers = [
      nextClient.on('backend.ready', () => setWsStatus('connected')),
      nextClient.on('devices.changed', (payload) => setDeviceList(payload as DeviceList)),
      nextClient.on('recording.status', (payload) => setRecording(payload as RecordingStatus)),
      nextClient.on('log', (payload) => appendLog(payload as BackendLogEvent)),
      nextClient.on('error', (payload) => {
        const error = payload as { message?: string }
        setLastError(error.message ?? 'Backend error.')
      }),
      nextClient.on('connection.closed', () => setWsStatus('closed'))
    ]

    nextClient
      .connect()
      .then(async () => {
        setWsStatus('connected')
        const nextHealth = await nextClient.request<BackendHealth>('health.ping', requestParams)
        setHealth(nextHealth)
        const nextDevices = await nextClient.request<DeviceList>('devices.list', requestParams)
        setDeviceList(nextDevices)
        const nextRecording = await nextClient.request<RecordingStatus>('recording.status')
        setRecording(nextRecording)
      })
      .catch((error: unknown) => {
        setWsStatus('failed')
        setLastError(error instanceof Error ? error.message : String(error))
      })

    return () => {
      for (const unsubscribe of unsubscribers) {
        unsubscribe()
      }
      nextClient.close()
      setClient(null)
    }
  }, [appendLog, connection, requestParams])

  const refreshBackend = useCallback(async () => {
    if (!client) {
      return
    }

    try {
      setLastError(null)
      const [nextHealth, nextDevices] = await Promise.all([
        client.request<BackendHealth>('health.ping', requestParams),
        client.request<DeviceList>('devices.list', requestParams)
      ])
      setHealth(nextHealth)
      setDeviceList(nextDevices)
    } catch (error) {
      setLastError(error instanceof Error ? error.message : String(error))
    }
  }, [client, requestParams])

  const startRecording = useCallback(async () => {
    if (!client) {
      return
    }

    try {
      setLastError(null)
      const status = await client.request<RecordingStatus>('recording.start_test', requestParams)
      setRecording(status)
    } catch (error) {
      setLastError(error instanceof Error ? error.message : String(error))
    }
  }, [client, requestParams])

  const stopRecording = useCallback(async () => {
    if (!client) {
      return
    }

    try {
      setLastError(null)
      const status = await client.request<RecordingStatus>('recording.stop')
      setRecording(status)
    } catch (error) {
      setLastError(error instanceof Error ? error.message : String(error))
    }
  }, [client])

  const canStart = wsStatus === 'connected' && !['recording', 'starting', 'stopping'].includes(recording.state)
  const canStop = wsStatus === 'connected' && ['recording', 'starting'].includes(recording.state)

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <div className="brand-row">
            <Radio aria-hidden="true" size={26} />
            <h1>Videogre</h1>
          </div>
          <p className="subhead">Recording studio spike</p>
        </div>
        <div className="topbar-actions">
          <StatusPill label="Backend" value={connection ? `${connection.host}:${connection.port}` : 'launching'} />
          <StatusPill label="Socket" value={wsStatus} tone={wsStatus === 'connected' ? 'good' : 'warn'} />
          <button className="icon-button" type="button" onClick={refreshBackend} title="Refresh backend">
            <RefreshCcw size={18} />
          </button>
        </div>
      </header>

      <section className="studio-grid">
        <Panel className="control-panel" title="Session" icon={Activity}>
          <div className="recording-state">
            <span className={`record-dot ${recording.state}`} />
            <div>
              <strong>{recording.state}</strong>
              <span>{recording.message ?? 'Idle'}</span>
            </div>
          </div>

          <div className="transport-row">
            <button className="primary-action" type="button" disabled={!canStart} onClick={startRecording}>
              <Play size={18} />
              Start test recording
            </button>
            <button className="secondary-action" type="button" disabled={!canStop} onClick={stopRecording}>
              <CircleStop size={18} />
              Stop
            </button>
          </div>

          <div className="output-box">
            <Folder aria-hidden="true" size={18} />
            <span>{recording.outputPath ?? 'Output path appears after recording starts.'}</span>
          </div>

          {lastError ? (
            <div className="notice error">
              <AlertTriangle aria-hidden="true" size={18} />
              <span>{lastError}</span>
            </div>
          ) : null}
        </Panel>

        <Panel title="Settings" icon={Settings}>
          <label className="field">
            <span>Output directory</span>
            <input
              value={settings.outputDirectory}
              placeholder="~/Movies/Videogre/Recordings"
              onChange={(event) => setSettings((current) => ({ ...current, outputDirectory: event.target.value }))}
            />
          </label>
          <label className="field">
            <span>FFmpeg path</span>
            <input
              value={settings.ffmpegPath}
              placeholder="ffmpeg"
              onChange={(event) => setSettings((current) => ({ ...current, ffmpegPath: event.target.value }))}
            />
          </label>
          <div className={`tool-status ${health?.ffmpeg.available ? 'good' : 'warn'}`}>
            {health?.ffmpeg.available ? <CheckCircle2 size={18} /> : <AlertTriangle size={18} />}
            <span>{health?.ffmpeg.version ?? health?.ffmpeg.message ?? 'Waiting for FFmpeg status.'}</span>
          </div>
        </Panel>

        <Panel className="devices-panel" title="Sources" icon={Monitor}>
          {deviceList.warnings.map((warning) => (
            <div className="notice warn" key={warning}>
              <AlertTriangle aria-hidden="true" size={18} />
              <span>{warning}</span>
            </div>
          ))}
          <div className="device-list">
            {deviceList.devices.length === 0 ? (
              <div className="empty-state">Waiting for device metadata.</div>
            ) : (
              deviceList.devices.map((device) => <DeviceRow device={device} key={device.id} />)
            )}
          </div>
        </Panel>

        <Panel className="logs-panel" title="Backend Log" icon={Activity}>
          <div className="log-list">
            {logs.length === 0 ? (
              <div className="empty-state">Waiting for backend logs.</div>
            ) : (
              logs.map((log, index) => (
                <div className={`log-line ${log.level}`} key={`${log.timestamp}-${index}`}>
                  <time>{compactTime(log.timestamp)}</time>
                  <span>{log.level}</span>
                  <p>{log.message}</p>
                </div>
              ))
            )}
          </div>
        </Panel>
      </section>
    </main>
  )
}

function Panel({
  children,
  className,
  icon: Icon,
  title
}: {
  children: ReactNode
  className?: string
  icon: typeof Activity
  title: string
}): ReactElement {
  return (
    <section className={`panel ${className ?? ''}`}>
      <header className="panel-header">
        <Icon aria-hidden="true" size={18} />
        <h2>{title}</h2>
      </header>
      {children}
    </section>
  )
}

function StatusPill({
  label,
  tone = 'neutral',
  value
}: {
  label: string
  tone?: 'good' | 'warn' | 'neutral'
  value: string
}): ReactElement {
  return (
    <div className={`status-pill ${tone}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  )
}

function DeviceRow({ device }: { device: Device }): ReactElement {
  const Icon = deviceIcons[device.kind]
  const isAvailable = device.status === 'available'

  return (
    <article className="device-row">
      <div className="device-icon">
        <Icon aria-hidden="true" size={20} />
      </div>
      <div className="device-copy">
        <strong>{device.name}</strong>
        <span>{device.detail ?? device.kind}</span>
      </div>
      <span className={`device-status ${isAvailable ? 'good' : 'warn'}`}>{device.status}</span>
    </article>
  )
}

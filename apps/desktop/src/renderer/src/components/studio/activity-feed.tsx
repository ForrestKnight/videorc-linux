import { useEffect, useRef, useState, type ReactElement } from 'react'

import { PanelSection } from '@/components/panel-section'
import { Button } from '@/components/ui/button'
import { useWorkspaceNav } from '@/components/workspace-nav'
import { useStudio } from '@/hooks/use-studio'
import { diffActivity, relativeAgo, type ActivitySnapshot } from '@/lib/studio-activity'

type ActivityEntry = { id: number; label: string; at: number }

const MAX_ENTRIES = 8

/**
 * Activity feed (SD4): a lightweight, renderer-only log of recent state
 * transitions (session start/stop, device connect/disconnect, capture-config
 * changes). There is no backend activity stream, so it is derived from
 * observable state via diffActivity — never fabricated. Relative times are
 * rendered without a polling tick (slightly stale is fine; no idle cost);
 * "View all" deep-links to Health.
 */
export function ActivityFeed(): ReactElement {
  const { recording, captureConfig, deviceList } = useStudio()
  const { setActive } = useWorkspaceNav()

  const deviceKey = deviceList.devices.map((device) => device.id).join(',')
  const recordingState = recording.state
  const { video, layout, audio, sources } = captureConfig

  const prevRef = useRef<ActivitySnapshot | null>(null)
  const idRef = useRef(0)
  const [entries, setEntries] = useState<ActivityEntry[]>([])

  useEffect(() => {
    const snapshot: ActivitySnapshot = {
      recordingState,
      deviceIds: deviceKey ? deviceKey.split(',') : [],
      videoPreset: video.preset,
      layoutPreset: layout.layoutPreset,
      microphoneMuted: audio.microphoneMuted,
      screenSourceKey: sources.screenId ?? sources.windowId ?? '',
      cameraId: sources.cameraId ?? '',
      microphoneId: sources.microphoneId ?? ''
    }
    const prev = prevRef.current
    prevRef.current = snapshot
    if (prev === null) {
      return // first render establishes the baseline; don't log pre-existing state
    }
    const labels = diffActivity(prev, snapshot)
    if (labels.length === 0) {
      return
    }
    const at = Date.now()
    setEntries((current) =>
      [...labels.map((label) => ({ id: idRef.current++, label, at })), ...current].slice(
        0,
        MAX_ENTRIES
      )
    )
  }, [
    recordingState,
    deviceKey,
    video.preset,
    layout.layoutPreset,
    audio.microphoneMuted,
    sources.screenId,
    sources.windowId,
    sources.cameraId,
    sources.microphoneId
  ])

  const now = Date.now()

  return (
    <PanelSection
      title="Activity"
      action={
        <Button size="sm" variant="ghost" onClick={() => setActive('diagnostics')}>
          View all
        </Button>
      }
    >
      {entries.length === 0 ? (
        <p className="text-sm text-muted-foreground">Recent changes will appear here.</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {entries.map((entry) => (
            <li className="flex items-center justify-between gap-3 text-sm" key={entry.id}>
              <span className="flex min-w-0 items-center gap-2">
                <span className="size-1.5 shrink-0 rounded-full bg-primary/60" />
                <span className="truncate">{entry.label}</span>
              </span>
              <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
                {relativeAgo(now - entry.at)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </PanelSection>
  )
}

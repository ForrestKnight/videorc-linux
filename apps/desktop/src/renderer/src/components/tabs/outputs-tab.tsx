import { Broadcast, FileVideo } from '@phosphor-icons/react'
import type { ReactElement } from 'react'

import { PanelSection } from '@/components/panel-section'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { useStudio } from '@/hooks/use-studio'
import type { RtmpPreset, VideoPreset } from '@/lib/backend'
import { formatDroppedFrames, formatMetric } from '@/lib/format'

export function OutputsTab(): ReactElement {
  const {
    captureConfig,
    setCaptureConfig,
    patchVideo,
    applyVideoPreset,
    applyRtmpPreset,
    streamHealth,
    streamReady
  } = useStudio()
  const { video } = captureConfig

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <PanelSection
        action={
          <Switch
            aria-label="Record MKV"
            checked={captureConfig.recordEnabled}
            onCheckedChange={(checked) => setCaptureConfig((current) => ({ ...current, recordEnabled: checked }))}
          />
        }
        description="Local recording is the primary output. Files are written as MKV, then optionally remuxed to MP4."
        icon={FileVideo}
        title="Recording"
      >
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="video-preset">Video preset</FieldLabel>
            <Select value={video.preset} onValueChange={(value) => applyVideoPreset(value as VideoPreset)}>
              <SelectTrigger className="w-full" id="video-preset">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="tutorial-1440p30">Tutorial 1440p30</SelectItem>
                  <SelectItem value="tutorial-1080p30">Tutorial 1080p30</SelectItem>
                  <SelectItem value="stream-1080p60">Stream 1080p60</SelectItem>
                  <SelectItem value="custom">Custom</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
            <FieldDescription>Editing a value below switches the preset to Custom.</FieldDescription>
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <NumberField
              label="Width"
              max={3840}
              min={640}
              value={video.width}
              onChange={(width) => patchVideo({ width })}
            />
            <NumberField
              label="Height"
              max={2160}
              min={360}
              value={video.height}
              onChange={(height) => patchVideo({ height })}
            />
            <NumberField label="FPS" max={60} min={24} value={video.fps} onChange={(fps) => patchVideo({ fps })} />
            <NumberField
              label="Bitrate kbps"
              max={50000}
              min={1000}
              step={500}
              value={video.bitrateKbps}
              onChange={(bitrateKbps) => patchVideo({ bitrateKbps })}
            />
          </div>
        </FieldGroup>
      </PanelSection>

      <PanelSection
        action={
          <Switch
            aria-label="Stream RTMP"
            checked={captureConfig.streamEnabled}
            onCheckedChange={(checked) => setCaptureConfig((current) => ({ ...current, streamEnabled: checked }))}
          />
        }
        description="Optional RTMP companion output. Stream keys are stored only here, never shown in Studio."
        icon={Broadcast}
        title="Streaming"
      >
        {captureConfig.streamEnabled && !streamReady ? (
          <Alert variant="warning">
            <Broadcast weight="fill" />
            <AlertTitle>Stream target incomplete</AlertTitle>
            <AlertDescription>An RTMP server and stream key are required before a session can start.</AlertDescription>
          </Alert>
        ) : null}

        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="rtmp-preset">RTMP preset</FieldLabel>
            <Select value={captureConfig.rtmpPreset} onValueChange={(value) => applyRtmpPreset(value as RtmpPreset)}>
              <SelectTrigger className="w-full" id="rtmp-preset">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="youtube">YouTube</SelectItem>
                  <SelectItem value="twitch">Twitch</SelectItem>
                  <SelectItem value="x">X / Twitter</SelectItem>
                  <SelectItem value="custom">Custom RTMP</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor="rtmp-server">RTMP server</FieldLabel>
            <Input
              id="rtmp-server"
              placeholder="rtmp://server/app"
              value={captureConfig.rtmpServerUrl}
              onChange={(event) =>
                setCaptureConfig((current) => ({ ...current, rtmpServerUrl: event.target.value }))
              }
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="stream-key">Stream key</FieldLabel>
            <Input
              autoComplete="off"
              id="stream-key"
              placeholder="manual stream key"
              type="password"
              value={captureConfig.streamKey}
              onChange={(event) => setCaptureConfig((current) => ({ ...current, streamKey: event.target.value }))}
            />
            <FieldDescription>Kept locally and only sent to the RTMP server when streaming.</FieldDescription>
          </Field>
        </FieldGroup>

        <div className="grid grid-cols-3 gap-2 text-center">
          <Metric label="FPS" value={formatMetric(streamHealth?.fps, 'fps')} />
          <Metric label="Dropped" value={formatDroppedFrames(streamHealth?.droppedFrames)} />
          <Metric label="Speed" value={formatMetric(streamHealth?.speed, 'x')} />
        </div>
      </PanelSection>
    </div>
  )
}

function NumberField({
  label,
  value,
  min,
  max,
  step,
  onChange
}: {
  label: string
  value: number
  min: number
  max: number
  step?: number
  onChange: (value: number) => void
}): ReactElement {
  return (
    <Field>
      <FieldLabel>{label}</FieldLabel>
      <Input
        max={max}
        min={min}
        step={step}
        type="number"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </Field>
  )
}

function Metric({ label, value }: { label: string; value: string }): ReactElement {
  return (
    <div className="rounded-lg border bg-muted/40 px-2 py-1.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="text-sm font-semibold tabular-nums">{value}</div>
    </div>
  )
}

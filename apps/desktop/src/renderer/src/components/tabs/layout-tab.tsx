import { FrameCorners, Layout, SlidersHorizontal } from '@phosphor-icons/react'
import type { ReactElement } from 'react'

import { PanelSection } from '@/components/panel-section'
import { PreviewStage } from '@/components/preview-stage'
import { Badge } from '@/components/ui/badge'
import { Field, FieldContent, FieldLabel } from '@/components/ui/field'
import { Label } from '@/components/ui/label'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { useStudio } from '@/hooks/use-studio'
import type { CameraCorner, CameraFit, CameraShape, CameraSize } from '@/lib/backend'

const LAYOUT_PRESETS = [
  { id: 'screen-camera', label: 'Screen + camera', enabled: true },
  { id: 'screen-only', label: 'Screen only', enabled: false },
  { id: 'camera-only', label: 'Camera only', enabled: false },
  { id: 'side-by-side', label: 'Side-by-side', enabled: false }
] as const

export function LayoutTab(): ReactElement {
  const { captureConfig, patchLayout, previewUrl, previewLoading, refreshPreview } = useStudio()
  const layout = captureConfig.layout

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]">
      <div className="flex flex-col gap-4">
        <PanelSection
          description="Only the screen/window + camera corner layout is enabled in v1."
          icon={Layout}
          title="Layout preset"
        >
          <div className="flex flex-wrap gap-2">
            {LAYOUT_PRESETS.map((preset) => (
              <button
                aria-pressed={preset.enabled}
                className="cursor-default rounded-xl border bg-card p-3 text-left text-sm font-medium transition-colors aria-pressed:border-primary aria-pressed:bg-primary/10 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={!preset.enabled}
                key={preset.id}
                type="button"
              >
                <div>{preset.label}</div>
                {!preset.enabled ? (
                  <Badge className="mt-1.5" variant="outline">
                    Soon
                  </Badge>
                ) : null}
              </button>
            ))}
          </div>
        </PanelSection>

        <PanelSection icon={FrameCorners} title="Preview">
          <PreviewStage
            layout={layout}
            onRefresh={refreshPreview}
            previewLoading={previewLoading}
            previewUrl={previewUrl}
          />
        </PanelSection>
      </div>

      <PanelSection icon={SlidersHorizontal} title="Camera framing">
        <Field>
          <FieldLabel>Corner</FieldLabel>
          <ToggleGroup
            className="w-full"
            type="single"
            value={layout.cameraCorner}
            variant="outline"
            onValueChange={(value) => value && patchLayout({ cameraCorner: value as CameraCorner })}
          >
            <ToggleGroupItem value="top-left">Top L</ToggleGroupItem>
            <ToggleGroupItem value="top-right">Top R</ToggleGroupItem>
            <ToggleGroupItem value="bottom-left">Bot L</ToggleGroupItem>
            <ToggleGroupItem value="bottom-right">Bot R</ToggleGroupItem>
          </ToggleGroup>
        </Field>

        <div className="grid grid-cols-2 gap-4">
          <Field>
            <FieldLabel>Size</FieldLabel>
            <ToggleGroup
              type="single"
              value={layout.cameraSize}
              variant="outline"
              onValueChange={(value) => value && patchLayout({ cameraSize: value as CameraSize })}
            >
              <ToggleGroupItem value="small">S</ToggleGroupItem>
              <ToggleGroupItem value="medium">M</ToggleGroupItem>
              <ToggleGroupItem value="large">L</ToggleGroupItem>
            </ToggleGroup>
          </Field>
          <Field>
            <FieldLabel>Shape</FieldLabel>
            <ToggleGroup
              type="single"
              value={layout.cameraShape}
              variant="outline"
              onValueChange={(value) => value && patchLayout({ cameraShape: value as CameraShape })}
            >
              <ToggleGroupItem value="rectangle">Rect</ToggleGroupItem>
              <ToggleGroupItem value="circle">Circle</ToggleGroupItem>
            </ToggleGroup>
          </Field>
        </div>

        <Field>
          <FieldLabel>Fit</FieldLabel>
          <ToggleGroup
            type="single"
            value={layout.cameraFit}
            variant="outline"
            onValueChange={(value) => value && patchLayout({ cameraFit: value as CameraFit })}
          >
            <ToggleGroupItem value="fill">Fill crop</ToggleGroupItem>
            <ToggleGroupItem value="fit">Fit frame</ToggleGroupItem>
          </ToggleGroup>
        </Field>

        <Field orientation="horizontal">
          <FieldContent>
            <FieldLabel htmlFor="camera-mirror">Mirror camera</FieldLabel>
          </FieldContent>
          <Switch
            checked={layout.cameraMirror}
            id="camera-mirror"
            onCheckedChange={(checked) => patchLayout({ cameraMirror: checked })}
          />
        </Field>

        <SliderField
          label="Margin"
          max={96}
          min={8}
          step={1}
          suffix="px"
          value={layout.cameraMargin}
          onChange={(cameraMargin) => patchLayout({ cameraMargin })}
        />
        <SliderField
          label="Zoom"
          max={200}
          min={100}
          step={5}
          suffix="%"
          value={layout.cameraZoom}
          onChange={(cameraZoom) => patchLayout({ cameraZoom })}
        />
        <SliderField
          label="Pan X"
          max={100}
          min={-100}
          step={5}
          value={layout.cameraOffsetX}
          onChange={(cameraOffsetX) => patchLayout({ cameraOffsetX })}
        />
        <SliderField
          label="Pan Y"
          max={100}
          min={-100}
          step={5}
          value={layout.cameraOffsetY}
          onChange={(cameraOffsetY) => patchLayout({ cameraOffsetY })}
        />
      </PanelSection>
    </div>
  )
}

function SliderField({
  label,
  value,
  min,
  max,
  step,
  suffix = '',
  onChange
}: {
  label: string
  value: number
  min: number
  max: number
  step: number
  suffix?: string
  onChange: (value: number) => void
}): ReactElement {
  return (
    <Field>
      <div className="flex items-center justify-between">
        <Label>{label}</Label>
        <span className="text-sm font-medium tabular-nums text-muted-foreground">
          {value}
          {suffix}
        </span>
      </div>
      <Slider max={max} min={min} step={step} value={[value]} onValueChange={([next]) => onChange(next)} />
    </Field>
  )
}

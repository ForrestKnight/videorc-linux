import { ArrowSquareOut, Monitor, VideoCamera } from '@phosphor-icons/react'
import type { ReactElement } from 'react'

import { Button } from '@/components/ui/button'
import type { Scene, SceneSource } from '@/lib/backend'
import { cn } from '@/lib/utils'

// SC1 (Scene rework): a pure-SVG schematic of the committed composition. The
// Scene tab used to make you edit transforms BLIND — the live preview is a
// detached window by design (idle-perf law: no always-on compositing in tabs),
// so this diagram renders the real normalized transforms with zero IPC cost.
// It is deliberately a diagram, not pixels; "Open preview" is the ground truth.

const STAGE_W = 160
const STAGE_H = 90

export function SceneStage({
  scene,
  selectedSourceId,
  hasBackground,
  previewOpen,
  onSelectSource,
  onTogglePreview
}: {
  scene: Scene | null
  selectedSourceId: string | null
  hasBackground: boolean
  previewOpen: boolean
  onSelectSource: (sourceId: string) => void
  onTogglePreview: () => void
}): ReactElement {
  const sources = scene?.sources ?? []

  return (
    <div className="relative overflow-hidden rounded-row border border-border bg-muted/20">
      <svg
        aria-label="Scene composition diagram"
        className="block w-full"
        role="img"
        viewBox={`0 0 ${STAGE_W} ${STAGE_H}`}
      >
        {/* Canvas */}
        <rect
          className={cn(hasBackground ? 'fill-primary/10' : 'fill-transparent')}
          height={STAGE_H}
          width={STAGE_W}
          x={0}
          y={0}
        />
        {sources.map((source) => (
          <StageSourceRect
            key={source.id}
            selected={source.id === selectedSourceId}
            source={source}
            onSelect={() => onSelectSource(source.id)}
          />
        ))}
        {sources.length === 0 ? (
          <text
            className="fill-muted-foreground"
            fontSize={5}
            textAnchor="middle"
            x={STAGE_W / 2}
            y={STAGE_H / 2}
          >
            No sources in the scene yet
          </text>
        ) : null}
      </svg>

      {/* Legend chips (HTML overlay, top-left) */}
      <div className="pointer-events-none absolute left-2 top-2 flex gap-1.5">
        {sources.map((source) => (
          <button
            key={source.id}
            className={cn(
              'pointer-events-auto flex items-center gap-1 rounded-chip border px-1.5 py-0.5 text-[11px] backdrop-blur-sm transition-colors',
              source.id === selectedSourceId
                ? 'border-ring bg-accent text-foreground'
                : 'border-border bg-background/70 text-muted-foreground hover:text-foreground',
              !source.visible && 'opacity-50'
            )}
            type="button"
            onClick={() => onSelectSource(source.id)}
          >
            {source.kind === 'camera' ? (
              <VideoCamera className="size-3" weight="duotone" />
            ) : (
              <Monitor className="size-3" weight="duotone" />
            )}
            <span className="max-w-24 truncate">{source.name}</span>
          </button>
        ))}
      </div>

      {/* Ground truth lives in the detached preview window. */}
      <div className="absolute inset-x-0 bottom-2 flex justify-center">
        <Button size="sm" variant="secondary" onClick={onTogglePreview}>
          <ArrowSquareOut data-icon="inline-start" />
          {previewOpen ? 'Close preview' : 'Open preview'}
        </Button>
      </div>
    </div>
  )
}

function StageSourceRect({
  source,
  selected,
  onSelect
}: {
  source: SceneSource
  selected: boolean
  onSelect: () => void
}): ReactElement {
  const x = source.transform.x * STAGE_W
  const y = source.transform.y * STAGE_H
  const width = Math.max(2, source.transform.width * STAGE_W)
  const height = Math.max(2, source.transform.height * STAGE_H)
  const camera = source.kind === 'camera'

  return (
    <g
      className="cursor-pointer"
      data-videorc-stage-source={source.id}
      onClick={(event) => {
        event.stopPropagation()
        onSelect()
      }}
    >
      <rect
        className={cn(
          camera ? 'fill-primary/25' : 'fill-muted-foreground/15',
          selected ? 'stroke-ring' : camera ? 'stroke-primary/60' : 'stroke-muted-foreground/50',
          !source.visible && 'opacity-40'
        )}
        height={height}
        rx={1.5}
        strokeDasharray={source.visible ? undefined : '2 1.5'}
        strokeWidth={selected ? 1.2 : 0.6}
        width={width}
        x={x}
        y={y}
      />
      {/* Label only when the rect is big enough to hold it. */}
      {width > 24 && height > 10 ? (
        <text
          className={cn('select-none', camera ? 'fill-primary' : 'fill-muted-foreground')}
          fontSize={4.2}
          x={x + 2.5}
          y={y + 5.5}
        >
          {source.name}
        </text>
      ) : null}
    </g>
  )
}

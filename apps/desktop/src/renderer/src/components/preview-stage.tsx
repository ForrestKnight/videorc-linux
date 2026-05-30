import { ArrowsClockwise, Image } from '@phosphor-icons/react'
import type { CSSProperties, ReactElement } from 'react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { LayoutSettings } from '@/lib/backend'
import { cn } from '@/lib/utils'

const SIZE_FRACTION: Record<LayoutSettings['cameraSize'], string> = {
  small: '20%',
  medium: '26%',
  large: '34%'
}

function cameraBoxStyle(layout: LayoutSettings): CSSProperties {
  const margin = `${layout.cameraMargin / 16}rem`
  const style: CSSProperties = {
    width: SIZE_FRACTION[layout.cameraSize],
    aspectRatio: '16 / 9',
    position: 'absolute'
  }

  if (layout.cameraCorner.includes('top')) {
    style.top = margin
  } else {
    style.bottom = margin
  }
  if (layout.cameraCorner.includes('left')) {
    style.left = margin
  } else {
    style.right = margin
  }

  return style
}

export function PreviewStage({
  previewUrl,
  previewLoading,
  layout,
  onRefresh,
  className
}: {
  previewUrl: string | null
  previewLoading: boolean
  layout: LayoutSettings
  onRefresh?: () => void
  className?: string
}): ReactElement {
  return (
    <div className={cn('flex flex-col gap-3', className)}>
      <div className="relative aspect-video w-full overflow-hidden rounded-xl border bg-muted">
        {previewUrl ? (
          <img alt="Selected scene preview" className="size-full object-contain" src={previewUrl} />
        ) : (
          <div className="flex size-full items-center justify-center">
            <Image className="size-10 text-muted-foreground/50" weight="duotone" />
            <div
              className={cn(
                'border-2 border-primary/60 bg-primary/10',
                layout.cameraShape === 'circle' ? 'rounded-full' : 'rounded-md'
              )}
              style={cameraBoxStyle(layout)}
            />
          </div>
        )}
        {previewLoading ? (
          <Badge className="absolute top-2 left-2" variant="secondary">
            Refreshing
          </Badge>
        ) : null}
      </div>
      {onRefresh ? (
        <Button className="self-start" size="sm" variant="outline" onClick={onRefresh}>
          <ArrowsClockwise data-icon="inline-start" />
          Refresh preview
        </Button>
      ) : null}
    </div>
  )
}

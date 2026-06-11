import { VideoCamera, Warning } from '@phosphor-icons/react'
import type { ReactElement } from 'react'

import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { useStudio } from '@/hooks/use-studio'
import type { PreviewLiveStatus, PreviewSurfaceStatus } from '@/lib/backend'
import { cn } from '@/lib/utils'

type PreviewStageProps = {
  previewLiveStatus?: PreviewLiveStatus
  previewSurfaceStatus?: PreviewSurfaceStatus
  nativePreviewSurfaceEnabled?: boolean
  onRetry?: () => void
  onOpenPermissions?: () => void
  className?: string
}

export function PreviewStage({
  previewLiveStatus,
  previewSurfaceStatus,
  nativePreviewSurfaceEnabled = false,
  onRetry,
  onOpenPermissions,
  className
}: PreviewStageProps): ReactElement {
  const { previewWindow, openPreviewWindow, closePreviewWindow, setPreviewWindowAlwaysOnTop } =
    useStudio()

  return (
    <DetachedPreviewCard
      alwaysOnTop={previewWindow.alwaysOnTop}
      className={className}
      nativePreviewSurfaceEnabled={nativePreviewSurfaceEnabled}
      previewLiveStatus={previewLiveStatus}
      previewSurfaceStatus={previewSurfaceStatus}
      previewWindowOpen={previewWindow.open}
      onAlwaysOnTopChange={(alwaysOnTop) => void setPreviewWindowAlwaysOnTop(alwaysOnTop)}
      onClose={() => void closePreviewWindow()}
      onOpen={() => void openPreviewWindow()}
      onOpenPermissions={onOpenPermissions}
      onRetry={onRetry}
    />
  )
}

function DetachedPreviewCard({
  previewWindowOpen,
  previewSurfaceStatus,
  previewLiveStatus,
  nativePreviewSurfaceEnabled,
  alwaysOnTop,
  onAlwaysOnTopChange,
  onOpen,
  onClose,
  onRetry,
  onOpenPermissions,
  className
}: {
  previewWindowOpen: boolean
  previewSurfaceStatus?: PreviewSurfaceStatus
  previewLiveStatus?: PreviewLiveStatus
  nativePreviewSurfaceEnabled: boolean
  alwaysOnTop: boolean
  onAlwaysOnTopChange: (alwaysOnTop: boolean) => void
  onOpen: () => void
  onClose: () => void
  onRetry?: () => void
  onOpenPermissions?: () => void
  className?: string
}): ReactElement {
  const transportLabel = previewWindowOpen
    ? previewTransportLabel(
        previewSurfaceStatus?.transport ?? 'unavailable',
        previewSurfaceStatus?.backing
      )
    : null
  const disabledMessage =
    previewLiveStatus?.message ??
    previewSurfaceStatus?.message ??
    'Native preview surface is disabled.'

  return (
    <div
      className={cn(
        'flex w-full flex-col items-center justify-center gap-3 rounded-xl border border-dashed bg-muted/20 px-6 py-10 text-center',
        className
      )}
      data-videorc-preview-card
    >
      {nativePreviewSurfaceEnabled ? (
        <VideoCamera className="size-8 text-muted-foreground" weight="duotone" />
      ) : (
        <Warning className="size-8 text-warning" weight="duotone" />
      )}
      {nativePreviewSurfaceEnabled ? (
        previewWindowOpen ? (
          <>
            <div className="flex flex-col gap-1">
              <span className="text-sm font-medium">Preview is open in its own window</span>
              <span className="text-xs text-muted-foreground">
                Drag, resize, or close it anytime{transportLabel ? ` - ${transportLabel}` : ''}
              </span>
            </div>
            <div className="flex gap-2">
              <Button size="sm" variant="secondary" onClick={onOpen}>
                Focus window
              </Button>
              <Button size="sm" variant="outline" onClick={onClose}>
                Close preview
              </Button>
            </div>
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              <Switch checked={alwaysOnTop} size="sm" onCheckedChange={onAlwaysOnTopChange} />
              Keep on top of other apps
            </label>
          </>
        ) : (
          <>
            <div className="flex flex-col gap-1">
              <span className="text-sm font-medium">Preview lives in its own window</span>
              <span className="text-xs text-muted-foreground">
                Open it to watch the program output.
              </span>
            </div>
            <Button data-videorc-open-preview-window size="sm" onClick={onOpen}>
              Open preview
              <kbd className="ml-2 rounded bg-background/40 px-1.5 font-mono text-[10px]">
                Cmd+P
              </kbd>
            </Button>
          </>
        )
      ) : (
        <>
          <div className="flex flex-col gap-1">
            <span className="text-sm font-medium">Native preview is disabled</span>
            <span className="text-xs text-muted-foreground">{disabledMessage}</span>
          </div>
          <div className="flex flex-wrap justify-center gap-2">
            {onRetry ? (
              <Button size="sm" variant="outline" onClick={onRetry}>
                Retry preview
              </Button>
            ) : null}
            {onOpenPermissions ? (
              <Button size="sm" variant="outline" onClick={onOpenPermissions}>
                Open permissions
              </Button>
            ) : null}
          </div>
        </>
      )}
    </div>
  )
}

function previewTransportLabel(
  transport: PreviewLiveStatus['transport'],
  backing?: PreviewSurfaceStatus['backing']
): string | null {
  switch (transport) {
    case 'native-surface':
      return backing === 'cametal-layer' ? 'Native preview' : 'Surface proof'
    case 'electron-proof-surface':
      return 'Electron proof'
    case 'latest-jpeg-polling':
      return 'JPEG fallback'
    case 'mjpeg-stream':
      return 'MJPEG debug'
    default:
      return null
  }
}

import type { Icon } from '@phosphor-icons/react'
import type { ReactElement, ReactNode } from 'react'

import { cn } from '@/lib/utils'

/**
 * Glass card (videorc-design): a floating translucent panel — hairline ring,
 * a subtle surface lift, soft elevation, and generous padding so each section
 * reads as its own pane of glass. The single section treatment for every tab.
 */
export function PanelSection({
  title,
  description,
  icon: LeadingIcon,
  action,
  children,
  className,
  contentClassName
}: {
  title: string
  description?: ReactNode
  icon?: Icon
  action?: ReactNode
  children: ReactNode
  className?: string
  contentClassName?: string
}): ReactElement {
  return (
    <section
      className={cn(
        'glass-shine flex flex-col gap-5 rounded-panel border border-border bg-card/40 p-5 shadow-soft',
        className
      )}
      data-slot="panel-section"
    >
      <header className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <h3 className="flex items-center gap-2 text-base leading-none font-medium text-foreground">
            {LeadingIcon ? (
              <LeadingIcon className="size-4 text-muted-foreground" weight="duotone" />
            ) : null}
            {title}
          </h3>
          {description ? (
            <div className="text-[13px] text-muted-foreground">{description}</div>
          ) : null}
        </div>
        {action ? <div className="shrink-0">{action}</div> : null}
      </header>
      <div className={cn('flex flex-col gap-5', contentClassName)}>{children}</div>
    </section>
  )
}

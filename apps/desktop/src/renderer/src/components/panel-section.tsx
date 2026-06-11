import type { Icon } from '@phosphor-icons/react'
import type { ReactElement, ReactNode } from 'react'

import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from '@/components/ui/card'
import { cn } from '@/lib/utils'

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
    <Card className={cn('gap-4', className)}>
      <CardHeader className="gap-1">
        <CardTitle className="flex items-center gap-2 text-base font-semibold">
          {LeadingIcon ? (
            <LeadingIcon className="size-4 text-muted-foreground" weight="duotone" />
          ) : null}
          {title}
        </CardTitle>
        {description ? <CardDescription>{description}</CardDescription> : null}
        {action ? <CardAction>{action}</CardAction> : null}
      </CardHeader>
      <CardContent className={cn('flex flex-col gap-4', contentClassName)}>{children}</CardContent>
    </Card>
  )
}

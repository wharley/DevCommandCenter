'use client'

import * as React from 'react'
import * as ProgressPrimitive from '@radix-ui/react-progress'

import { cn } from '@/lib/utils'

interface ProgressProps extends React.ComponentProps<typeof ProgressPrimitive.Root> {
  variant?: 'default' | 'indeterminate'
}

function Progress({
  className,
  value,
  variant = 'default',
  ...props
}: ProgressProps) {
  const isIndeterminate = variant === 'indeterminate' || (variant === 'default' && value === undefined)
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      className={cn(
        'bg-primary/20 relative h-2 w-full overflow-hidden rounded-full',
        className,
      )}
      {...props}
    >
      <ProgressPrimitive.Indicator
        data-slot="progress-indicator"
        className={cn(
          'h-full flex-1',
          isIndeterminate
            ? 'bg-primary w-1/3 animate-[progress-indeterminate_1.5s_ease-in-out_infinite]'
            : 'bg-primary w-full transition-all',
        )}
        style={isIndeterminate ? undefined : { transform: `translateX(-${100 - (value || 0)}%)` }}
      />
    </ProgressPrimitive.Root>
  )
}

export { Progress }

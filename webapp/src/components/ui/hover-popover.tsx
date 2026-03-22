'use client'

import React from 'react'

interface HoverPopoverProps {
  children: React.ReactNode
  content: React.ReactNode
}

/**
 * Wraps `children` in a hover group. `content` appears in a styled popover
 * above the trigger. pb-1.5 on the popover wrapper bridges the visual gap so
 * the mouse doesn't leave the group when moving from trigger up to popover.
 */
export function HoverPopover({ children, content }: HoverPopoverProps) {
  return (
    <div className="relative group inline-block">
      {children}
      <div className="absolute bottom-full left-1/2 -translate-x-1/2 pb-1.5 z-10
                      invisible group-hover:visible opacity-0 group-hover:opacity-100
                      transition-opacity duration-150">
        <div className="bg-popover border border-border rounded-md shadow-md
                        px-2.5 py-1.5 whitespace-nowrap text-xs">
          {content}
        </div>
      </div>
    </div>
  )
}

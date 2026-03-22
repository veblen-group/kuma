'use client'

import { ExplorerLink } from '@/components/ui/explorer-link'
import { HoverPopover } from '@/components/ui/hover-popover'

interface BlockCellProps {
  chain: string
  height: number
  timestamp?: string | null
}

export function BlockCell({ chain, height, timestamp }: BlockCellProps) {
  const link = <ExplorerLink chain={chain} type="block" value={height} />

  if (!timestamp) return link

  return (
    <HoverPopover content={
      <span className="text-muted-foreground">
        {new Date(timestamp).toLocaleString()}
      </span>
    }>
      {link}
    </HoverPopover>
  )
}

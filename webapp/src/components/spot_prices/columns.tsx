"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { SpotPrice } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { HoverPopover } from "@/components/ui/hover-popover"
import { BlockCell } from "@/components/ui/block-cell"
import { ChainBadge } from "@/components/ui/chain-badge"
import { formatPrice } from "@/lib/token-config"

function PriceWithPool({ chain, price, poolId }: { chain: string; price: number; poolId: string }) {
  return (
    <HoverPopover content={
      <>
        <span className="text-muted-foreground mr-1.5">pool</span>
        <ExplorerLink chain={chain} type="address" value={poolId} />
      </>
    }>
      <span className="tabular-nums cursor-default underline decoration-dotted decoration-muted-foreground/50">
        {formatPrice(price)}
      </span>
    </HoverPopover>
  )
}

export const columns: ColumnDef<SpotPrice>[] = [
  {
    header: "Chain",
    accessorKey: "chain",
    cell: ({ row }) => <ChainBadge chain={row.getValue("chain") as string} />,
  },
  {
    header: "Block",
    accessorKey: "block_height",
    cell: ({ row }) => (
      <BlockCell
        chain={row.original.chain}
        height={row.getValue("block_height") as number}
        timestamp={row.original.created_at}
      />
    ),
  },
  {
    header: "Min Price",
    accessorKey: "min_price",
    cell: ({ row }) => (
      <PriceWithPool
        chain={row.original.chain}
        price={row.getValue("min_price") as number}
        poolId={row.original.min_pool_id}
      />
    ),
  },
  {
    header: "Max Price",
    accessorKey: "max_price",
    cell: ({ row }) => (
      <PriceWithPool
        chain={row.original.chain}
        price={row.getValue("max_price") as number}
        poolId={row.original.max_pool_id}
      />
    ),
  },
]

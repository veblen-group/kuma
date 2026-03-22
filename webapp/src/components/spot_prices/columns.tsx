"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { SpotPrice } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { formatPrice } from "@/lib/token-config"
import { ChainBadge } from "@/components/ui/chain-badge"

function PriceWithPool({
  chain,
  price,
  poolId,
}: {
  chain: string
  price: number
  poolId: string
}) {
  return (
    <div className="relative group inline-block">
      <span className="tabular-nums cursor-default underline decoration-dotted decoration-muted-foreground/50">
        {formatPrice(price)}
      </span>
      {/*
        pb-1.5 fills the gap below the popover box with transparent space that's
        still inside the group, so hover is maintained moving up from text to popover.
        Appears above to avoid being clipped by the table's overflow container.
      */}
      <div className="absolute bottom-full left-1/2 -translate-x-1/2 pb-1.5 z-10
                      invisible group-hover:visible opacity-0 group-hover:opacity-100
                      transition-opacity duration-150">
        <div className="bg-popover border border-border rounded-md shadow-md
                        px-2.5 py-1.5 whitespace-nowrap text-xs">
          <span className="text-muted-foreground mr-1.5">pool</span>
          <ExplorerLink chain={chain} type="address" value={poolId} />
        </div>
      </div>
    </div>
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
    cell: ({ row }) => {
      const createdAt = row.original.created_at
      const height = row.getValue("block_height") as number
      if (!createdAt) return <ExplorerLink chain={row.original.chain} type="block" value={height} />
      return (
        <div className="relative group inline-block">
          <ExplorerLink chain={row.original.chain} type="block" value={height} />
          <div className="absolute bottom-full left-1/2 -translate-x-1/2 pb-1.5 z-10
                          invisible group-hover:visible opacity-0 group-hover:opacity-100
                          transition-opacity duration-150">
            <div className="bg-popover border border-border rounded-md shadow-md
                            px-2.5 py-1.5 whitespace-nowrap text-xs text-muted-foreground">
              {new Date(createdAt).toLocaleString()}
            </div>
          </div>
        </div>
      )
    },
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

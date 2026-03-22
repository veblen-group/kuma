"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { SpotPrice } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { TokenBadge } from "@/components/ui/token-badge"
import { formatPrice } from "@/lib/token-config"
import { ChainBadge } from "@/components/ui/chain-badge"

export const columns: ColumnDef<SpotPrice>[] = [
  {
    header: "Created At",
    accessorKey: "created_at",
    cell: ({ row }) => {
      const raw = row.getValue("created_at") as string | null
      if (!raw) return "—"
      return new Date(raw).toLocaleString()
    },
  },
  {
    header: "Chain",
    accessorKey: "chain",
    cell: ({ row }) => <ChainBadge chain={row.getValue("chain") as string} />,
  },
  {
    header: "Block",
    accessorKey: "block_height",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.chain}
        type="block"
        value={row.getValue("block_height") as number}
      />
    ),
  },
  {
    header: "Token A",
    accessorKey: "pair_token_a",
    cell: ({ row }) => <TokenBadge symbol={row.getValue("pair_token_a") as string} />,
  },
  {
    header: "Token B",
    accessorKey: "pair_token_b",
    cell: ({ row }) => <TokenBadge symbol={row.getValue("pair_token_b") as string} />,
  },
  {
    header: "Min Pool",
    accessorKey: "min_pool_id",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.chain}
        type="address"
        value={row.getValue("min_pool_id") as string}
      />
    ),
  },
  {
    header: "Min Price",
    accessorKey: "min_price",
    cell: ({ row }) => formatPrice(row.getValue("min_price") as number),
  },
  {
    header: "Max Pool",
    accessorKey: "max_pool_id",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.chain}
        type="address"
        value={row.getValue("max_pool_id") as string}
      />
    ),
  },
  {
    header: "Max Price",
    accessorKey: "max_price",
    cell: ({ row }) => formatPrice(row.getValue("max_price") as number),
  },
]

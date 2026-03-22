"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { FailedOnSlowTrade } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { ChainBadge } from "@/components/ui/chain-badge"

export const columns: ColumnDef<FailedOnSlowTrade>[] = [
  {
    header: "Trade ID",
    accessorKey: "id",
  },
  {
    header: "Signal ID",
    accessorFn: (row) => row.signal.id,
    cell: ({ getValue }) => (
      <a href={`#signal-${getValue()}`} className="font-mono text-xs underline hover:text-primary">
        {getValue() as number}
      </a>
    ),
  },
  {
    header: "Slow Chain",
    id: "slow_chain",
    cell: ({ row }) => <ChainBadge chain={row.original.signal.slow.chain} />,
  },
  {
    header: "Fast Chain",
    id: "fast_chain",
    cell: ({ row }) => <ChainBadge chain={row.original.signal.fast.chain} />,
  },
  {
    header: "Slow Tx",
    id: "slow_tx",
    cell: ({ row }) => {
      const hash = row.original.slow_tx_hash
      if (!hash) return <span className="text-muted-foreground">—</span>
      return (
        <ExplorerLink chain={row.original.signal.slow.chain} type="tx" value={hash} />
      )
    },
  },
]

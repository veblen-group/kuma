"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { FailedOnFastTrade } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { ChainBadge } from "@/components/ui/chain-badge"

export const columns: ColumnDef<FailedOnFastTrade>[] = [
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
    accessorKey: "slow_tx_hash",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.signal.slow.chain}
        type="tx"
        value={row.getValue("slow_tx_hash") as string}
      />
    ),
  },
  {
    header: "Fast Tx",
    id: "fast_tx",
    cell: ({ row }) => {
      const hash = row.original.fast_tx_hash
      if (!hash) return <span className="text-muted-foreground">—</span>
      return (
        <ExplorerLink chain={row.original.signal.fast.chain} type="tx" value={hash} />
      )
    },
  },
]

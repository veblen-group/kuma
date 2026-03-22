"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { SuccessfulTrade } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { ChainBadge } from "@/components/ui/chain-badge"

export const columns: ColumnDef<SuccessfulTrade>[] = [
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
    accessorKey: "fast_tx_hash",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.signal.fast.chain}
        type="tx"
        value={row.getValue("fast_tx_hash") as string}
      />
    ),
  },
  {
    header: "Realized Profit",
    accessorKey: "realized_profit_str",
  },
]

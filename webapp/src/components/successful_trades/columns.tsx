"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { SuccessfulTrade } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { ChainBadge } from "@/components/ui/chain-badge"
import { TokenAmount } from "@/components/ui/token-amount"

export const columns: ColumnDef<SuccessfulTrade>[] = [
  {
    header: "Signal ID",
    accessorFn: (row) => row.signal.id,
    cell: ({ getValue }) => (
      <a href={`/signals/${getValue()}`} className="font-mono text-xs underline hover:text-primary">
        {getValue() as number}
      </a>
    ),
  },
  {
    header: "Slow Leg",
    id: "slow",
    cell: ({ row }) => (
      <div className="flex flex-col gap-1">
        <ChainBadge chain={row.original.signal.slow.chain} />
        <ExplorerLink chain={row.original.signal.slow.chain} type="tx" value={row.original.slow_tx_hash} />
      </div>
    ),
  },
  {
    header: "Fast Leg",
    id: "fast",
    cell: ({ row }) => (
      <div className="flex flex-col gap-1">
        <ChainBadge chain={row.original.signal.fast.chain} />
        <ExplorerLink chain={row.original.signal.fast.chain} type="tx" value={row.original.fast_tx_hash} />
      </div>
    ),
  },
  {
    header: "Realized Profit",
    accessorKey: "realized_profit_str",
    cell: ({ row }) => (
      <TokenAmount amount={row.original.realized_profit_str} symbol="USDC" />
    ),
  },
]

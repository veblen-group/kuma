"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { FailedOnSlowTrade } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { ChainBadge } from "@/components/ui/chain-badge"
import { FailureReasonBadge } from "@/components/ui/failure-reason-badge"

export const columns: ColumnDef<FailedOnSlowTrade>[] = [
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
        {row.original.slow_tx_hash
          ? <ExplorerLink chain={row.original.signal.slow.chain} type="tx" value={row.original.slow_tx_hash} />
          : <span className="text-muted-foreground text-xs">—</span>
        }
      </div>
    ),
  },
  {
    header: "Fast Leg",
    id: "fast",
    cell: ({ row }) => (
      <ChainBadge chain={row.original.signal.fast.chain} />
    ),
  },
  {
    header: "Failure Reason",
    id: "failure_reason",
    cell: ({ row }) => <FailureReasonBadge reason={row.original.failure_reason} />,
  },
]

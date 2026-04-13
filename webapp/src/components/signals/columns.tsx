"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { Signal, Swap } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { BlockCell } from "@/components/ui/block-cell"
import { TokenBadge } from "@/components/ui/token-badge"
import { TokenAmount } from "@/components/ui/token-amount"
import { ChainBadge } from "@/components/ui/chain-badge"
import { MoveRight } from "lucide-react"

function TokenPair({ tokenIn, tokenOut }: { tokenIn: string; tokenOut: string }) {
  return (
    <div className="flex items-center gap-1.5">
      <TokenBadge symbol={tokenIn} />
      <MoveRight size={12} className="shrink-0 text-muted-foreground" />
      <TokenBadge symbol={tokenOut} />
    </div>
  )
}

function LegCell({ swap, timestamp }: { swap: Swap; timestamp?: string | null }) {
  return (
    <div className="flex flex-col items-start gap-1">
      <ChainBadge chain={swap.chain} />
      <BlockCell chain={swap.chain} height={swap.height} timestamp={timestamp ?? undefined} />
    </div>
  )
}

function PairCell({ swap }: { swap: Swap }) {
  return (
    <div className="flex flex-col items-start gap-1">
      <TokenPair tokenIn={swap.token_in} tokenOut={swap.token_out} />
      <ExplorerLink chain={swap.chain} type="address" value={swap.pool_id} />
    </div>
  )
}

export const columns: ColumnDef<Signal>[] = [
  {
    header: "Signal ID",
    accessorKey: "id",
    cell: ({ getValue }) => (
      <a href={`/signals/${getValue()}`} className="font-mono text-xs underline hover:text-primary">
        {getValue() as number}
      </a>
    ),
  },
  {
    header: "Slow Chain",
    id: "slow_leg",
    cell: ({ row }) => (
      <LegCell swap={row.original.slow} timestamp={row.original.slow_prices_a_b?.created_at} />
    ),
  },
  {
    header: "Slow Pair",
    id: "slow_pair",
    cell: ({ row }) => <PairCell swap={row.original.slow} />,
  },
  {
    header: "Fast Chain",
    id: "fast_leg",
    cell: ({ row }) => (
      <LegCell swap={row.original.fast} timestamp={row.original.fast_prices_a_b_created_at} />
    ),
  },
  {
    header: "Fast Pair",
    id: "fast_pair",
    cell: ({ row }) => <PairCell swap={row.original.fast} />,
  },
  {
    header: "Min Profit",
    id: "min_profit",
    cell: ({ row }) => (
      <TokenAmount amount={row.original.expected_profit.min_total_amount_usdc} symbol="USDC" />
    ),
  },
]

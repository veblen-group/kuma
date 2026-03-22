"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { Signal } from "@/lib/types"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { TokenBadge } from "@/components/ui/token-badge"
import { formatTokenAmount, formatWithDecimals } from "@/lib/token-config"
import { ChainBadge } from "@/components/ui/chain-badge"

function TokenPair({ tokenIn, tokenOut }: { tokenIn: string; tokenOut: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <TokenBadge symbol={tokenIn} />
      <span className="text-muted-foreground text-xs">→</span>
      <TokenBadge symbol={tokenOut} />
    </span>
  )
}

export const columns: ColumnDef<Signal>[] = [
  {
    header: "Signal ID",
    accessorKey: "id",
    cell: ({ getValue }) => (
      <a href={`#signal-${getValue()}`} className="font-mono text-xs underline hover:text-primary">
        {getValue() as number}
      </a>
    ),
  },
  {
    header: "Slow Chain",
    id: "slow_chain",
    cell: ({ row }) => <ChainBadge chain={row.original.slow.chain} />,
  },
  {
    header: "Slow Block",
    id: "slow_block",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.slow.chain}
        type="block"
        value={row.original.slow.height}
      />
    ),
  },
  {
    header: "Slow Pair",
    id: "slow_pair",
    cell: ({ row }) => (
      <TokenPair tokenIn={row.original.slow.token_in} tokenOut={row.original.slow.token_out} />
    ),
  },
  {
    header: "Slow Pool",
    id: "slow_pool",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.slow.chain}
        type="address"
        value={row.original.slow.pool_id}
      />
    ),
  },
  {
    header: "Fast Chain",
    id: "fast_chain",
    cell: ({ row }) => <ChainBadge chain={row.original.fast.chain} />,
  },
  {
    header: "Fast Pair",
    id: "fast_pair",
    cell: ({ row }) => (
      <TokenPair tokenIn={row.original.fast.token_in} tokenOut={row.original.fast.token_out} />
    ),
  },
  {
    header: "Fast Pool",
    id: "fast_pool",
    cell: ({ row }) => (
      <ExplorerLink
        chain={row.original.fast.chain}
        type="address"
        value={row.original.fast.pool_id}
      />
    ),
  },
  {
    header: "Surplus A",
    id: "surplus_a",
    cell: ({ row }) => {
      const { surplus_a, token_a } = row.original.expected_profit
      return formatTokenAmount(surplus_a, token_a)
    },
  },
  {
    header: "Surplus B",
    id: "surplus_b",
    cell: ({ row }) => {
      const { surplus_b, token_b } = row.original.expected_profit
      return formatTokenAmount(surplus_b, token_b)
    },
  },
  {
    header: "Min Profit (USDC)",
    id: "min_profit",
    cell: ({ row }) => {
      // USDC has 6 decimals
      return formatWithDecimals(row.original.expected_profit.min_total_amount_usdc, 6)
    },
  },
]

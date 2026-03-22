"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { Signal } from "@/lib/types";

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
    accessorFn: (row) => row.slow.chain,
  },
  {
    header: "Slow Height",
    accessorFn: (row) => row.slow.height,
  },
  {
    header: "Slow Pair",
    accessorFn: (row) => `${row.slow.token_in}-${row.slow.token_out}`,
  },
  {
    header: "Slow Pool ID",
    accessorFn: (row) => row.slow.pool_id,
  },
  {
    header: "Fast Chain",
    accessorFn: (row) => row.fast.chain,
  },
  {
    header: "Fast Pair",
    accessorFn: (row) => `${row.fast.token_in}-${row.fast.token_out}`,
  },
  {
    header: "Fast Pool ID",
    accessorFn: (row) => row.fast.pool_id,
  },
  {
    header: "Surplus A",
    accessorFn: (row) => row.expected_profit.surplus_a,
  },
  {
    header: "Surplus B",
    accessorFn: (row) => row.expected_profit.surplus_b,
  },
  {
    header: "Min Profit (USDC)",
    accessorFn: (row) => row.expected_profit.min_total_amount_usdc,
  },
];

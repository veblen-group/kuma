"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { ArbitrageSignal } from "@/lib/types";

export const columns: ColumnDef<ArbitrageSignal>[] = [
  {
    header: "Block Height",
    accessorKey: "block_height",
  },
  {
    header: "Slow Chain",
    accessorKey: "slow_chain",
  },
  {
    header: "Slow Pair",
    accessorFn: (row) => `${row.slow_pair.token_a.symbol}-${row.slow_pair.token_b.symbol}`,
  },
  {
    header: "Slow Pool ID",
    accessorKey: "slow_pool_id",
  },
  {
    header: "Fast Chain",
    accessorKey: "fast_chain",
  },
  {
    header: "Fast Pair",
    accessorFn: (row) => `${row.fast_pair.token_a.symbol}-${row.fast_pair.token_b.symbol}`,
  },
  {
    header: "Fast Pool ID",
    accessorKey: "fast_pool_id",
  },
  {
    header: "Surplus A",
    accessorKey: "surplus_a",
  },
  {
    header: "Surplus B",
    accessorKey: "surplus_b",
  },
  {
    header: "Expected Profit A",
    accessorKey: "expected_profit_a",
  },
  {
    header: "Expected Profit B",
    accessorKey: "expected_profit_b",
  },
  {
    header: "Max Slippage (bps)",
    accessorKey: "max_slippage_bps",
  },
];

"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { Signal } from "@/lib/types";

export const columns: ColumnDef<Signal>[] = [
  {
    header: "Block Height",
    accessorKey: "block_height",
  },
  {
    header: "Slow Chain",
    accessorFn: (row) => row.slow_chain.name,
  },
  {
    header: "Slow Pair",
    accessorFn: (row) => `${row.slow_pair[0].symbol}-${row.slow_pair[1].symbol}`,
  },
  {
    header: "Slow Pool ID",
    accessorKey: "slow_pool_id",
  },
  {
    header: "Fast Chain",
    accessorFn: (row) => row.fast_chain.name,
  },
  {
    header: "Fast Pair",
    accessorFn: (row) => `${row.fast_pair[0].symbol}-${row.fast_pair[1].symbol}`,
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
];

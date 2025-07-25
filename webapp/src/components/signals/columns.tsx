"use client"

import type { ColumnDef } from "@tanstack/react-table";

export const data: CrossChainSingleHop[] = [
  {
    slow_chain: "Chain A",
    slow_pair: "Pair X",
    slow_height: 100,
    slow_id: "ID1",
    slow_sim: "Sim1",
    fast_chain: "Chain B",
    fast_pair: "Pair Y",
    fast_height: 200,
    fast_id: "ID2",
    fast_sim: "Sim2",
    surplus: ["10", "20"],
    expected_profit: ["30", "40"],
  },
];

export interface CrossChainSingleHop {
  slow_chain: string
  slow_pair: string
  slow_height: number
  slow_id: string
  slow_sim: string
  fast_chain: string
  fast_pair: string
  fast_height: number
  fast_id: string
  fast_sim: string
  surplus: [string, string]
  expected_profit: [string, string]
};

export const columns: ColumnDef<CrossChainSingleHop>[] = [
  {
    accessorKey: "slow_chain",
    header: "Slow Chain",
  },
  {
    accessorKey: "slow_pair",
    header: "Slow Pair",
  },
  {
    accessorKey: "slow_height",
    header: "Slow Height",
  },
  {
    accessorKey: "slow_id",
    header: "Slow ID",
  },
  {
    accessorKey: "slow_sim",
    header: "Slow Sim",
  },
  {
    accessorKey: "fast_chain",
    header: "Fast Chain",
  },
  {
    accessorKey: "fast_pair",
    header: "Fast Pair",
  },
  {
    accessorKey: "fast_height",
    header: "Fast Height",
  },
  {
    accessorKey: "fast_id",
    header: "Fast ID",
  },
  {
    accessorKey: "fast_sim",
    header: "Fast Sim",
  },
  {
    accessorKey: "surplus",
    header: "Surplus",
  },
  {
    accessorKey: "expected_profit",
    header: "Expected Profit",
  },
];

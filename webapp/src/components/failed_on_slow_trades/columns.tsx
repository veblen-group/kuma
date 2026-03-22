"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { FailedOnSlowTrade } from "@/lib/types";

export const columns: ColumnDef<FailedOnSlowTrade>[] = [
  {
    header: "Trade ID",
    accessorKey: "id",
  },
  {
    header: "Signal ID",
    accessorFn: (row) => row.signal.id,
  },
  {
    header: "Slow Chain",
    accessorFn: (row) => row.signal.slow.chain,
  },
  {
    header: "Fast Chain",
    accessorFn: (row) => row.signal.fast.chain,
  },
  {
    header: "Slow Tx Hash",
    accessorFn: (row) => row.slow_tx_hash ?? "—",
  },
];

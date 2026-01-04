"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { TradeResult } from "@/lib/types";

export const columns: ColumnDef<TradeResult>[] = [
  {
    header: "ID",
    accessorKey: "id",
    cell: ({ row }) => (
      <span className="font-mono text-xs">#{row.getValue("id")}</span>
    ),
  },
  {
    header: "Signal ID",
    accessorFn: (row) => row.signal_id,
  },
  {
    header: "Slow Chain",
    accessorFn: (row) => row.slow_chain,
  },
  {
    header: "Fast Chain",
    accessorFn: (row) => row.fast_chain,
  },
  {
    header: "Slow Tx Hash",
    accessorKey: "slow_tx_hash",
    cell: ({ row }) => {
      const hash = row.getValue("slow_tx_hash") as string;
      return (
        <span className="font-mono text-xs" title={hash}>
          {hash.slice(0, 8)}...{hash.slice(-6)}
        </span>
      );
    },
  }
];
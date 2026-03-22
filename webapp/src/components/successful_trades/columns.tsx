"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { SuccessfulTrade } from "@/lib/types";

export const columns: ColumnDef<SuccessfulTrade>[] = [
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
    accessorKey: "slow_tx_hash",
    cell: ({ row }) => {
      const hash = row.getValue("slow_tx_hash") as string;
      return (
        <span className="font-mono text-xs" title={hash}>
          {hash.slice(0, 8)}...{hash.slice(-6)}
        </span>
      );
    },
  },
  {
    header: "Fast Tx Hash",
    accessorKey: "fast_tx_hash",
    cell: ({ row }) => {
      const hash = row.getValue("fast_tx_hash") as string;
      return (
        <span className="font-mono text-xs" title={hash}>
          {hash.slice(0, 8)}...{hash.slice(-6)}
        </span>
      );
    },
  },
  {
    header: "Realized Profit",
    accessorKey: "realized_profit_str",
  },
];

"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { FailedOnFastTrade } from "@/lib/types";

export const columns: ColumnDef<FailedOnFastTrade>[] = [
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
    accessorFn: (row) => row.fast_tx_hash ?? "—",
  },
];

"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { SpotPrice } from "@/lib/types";

export const columns: ColumnDef<SpotPrice>[] = [
  {
    header: "Chain",
    accessorKey: "chain",
  },
  {
    header: "Block Height",
    accessorKey: "block_height",
  },
  {
    header: "Token A",
    accessorFn: (row) => row.pair.token_a.symbol,
  },
  {
    header: "Token B", 
    accessorFn: (row) => row.pair.token_b.symbol,
  },
  {
    header: "Price",
    accessorKey: "price",
  },
  {
    header: "Pool ID",
    accessorKey: "pool_id",
  },
];

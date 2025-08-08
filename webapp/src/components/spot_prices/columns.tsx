"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { SpotPrice } from "@/lib/types";

export const columns: ColumnDef<SpotPrice>[] = [
  {
    header: "Chain",
    accessorFn: (row) => row.chain.name,
  },
  {
    header: "Block Height",
    accessorKey: "block_height",
  },
  {
    header: "Token A",
    accessorFn: (row) => row.pair[0].symbol,
  },
  {
    header: "Token B",
    accessorFn: (row) => row.pair[1].symbol,
  },
  {
    header: "Min Pool ID",
    accessorKey: "min_pool_id",
  },
  {
    header: "Min Price",
    accessorKey: "min_price",
  },
  {
    header: "Max Pool ID",
    accessorKey: "max_pool_id",
  },
  {
    header: "Max Price",
    accessorKey: "max_price",
  },
];

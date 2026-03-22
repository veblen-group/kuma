"use client"

import type { ColumnDef } from "@tanstack/react-table";
import { SpotPrice } from "@/lib/types";

export const columns: ColumnDef<SpotPrice>[] = [
  {
    header: "ID",
    accessorKey: "id",
  },
  {
    header: "Created At",
    accessorKey: "created_at",
  },
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
    accessorKey: "pair_token_a",
  },
  {
    header: "Token B",
    accessorKey: "pair_token_b",
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

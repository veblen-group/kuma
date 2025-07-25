"use client"

import type { ColumnDef } from "@tanstack/react-table";

export const data: SpotPrice[] = [
  {
    chain: "Ethereum",
    block_height: 12345678,
    block_hash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    a_to_b: 1.23456789,
    b_to_a: 0.987654321,
  },
  {
    chain: "Base",
    block_height: 98765432,
    block_hash: "0x9876543210abcdef9876543210abcdef9876543210abcdef9876543210abcdef",
    a_to_b: 2.34567890,
    b_to_a: 0.87654321,
  },
];

export interface SpotPrice {
  chain: string;
  block_height: number;
  block_hash: string;
  a_to_b: number;
  b_to_a: number;
};

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
    header: "Block Hash",
    accessorKey: "block_hash",
  },
  {
    header: "A to B",
    accessorKey: "a_to_b",
  },
  {
    header: "B to A",
    accessorKey: "b_to_a",
  },
];

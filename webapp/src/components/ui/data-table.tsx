"use client"

import * as React from "react"
import {
  ColumnDef,
  flexRender,
  getCoreRowModel,
  getPaginationRowModel,
  useReactTable,
  Row,
} from "@tanstack/react-table"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Button } from "@/components/ui/button"
import { PaginatedResponse } from "@/lib/types"
import { useStablePageCount } from "@/lib/use-stable-page-count"

interface DataTableProps<TData> {
  columns: ColumnDef<TData>[]
  data: PaginatedResponse<TData> | undefined
  isLoading: boolean
  isError: boolean
  error: Error | null
  refetch: () => void
  pagination: { pageIndex: number; pageSize: number }
  onPaginationChange: React.Dispatch<React.SetStateAction<{ pageIndex: number; pageSize: number }>>
  emptyMessage: string
  loadingMessage?: string
  containerClassName?: string
  tableClassName?: string
  getRowId?: (row: TData) => string
}

export function DataTable<TData>({
  columns,
  data,
  isLoading,
  isError,
  error,
  refetch,
  pagination,
  onPaginationChange,
  emptyMessage,
  loadingMessage = "Loading...",
  containerClassName,
  tableClassName,
  getRowId,
}: DataTableProps<TData>) {
  const pageCount = useStablePageCount(data)

  const table = useReactTable({
    data: data?.data ?? [],
    columns,
    getCoreRowModel: getCoreRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    manualPagination: true,
    pageCount,
    onPaginationChange,
    state: { pagination },
    ...(getRowId ? { getRowId } : {}),
  })

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-24 text-muted-foreground">
        {loadingMessage}
      </div>
    )
  }

  if (isError) {
    return (
      <div className="flex items-center justify-center h-24 text-red-500">
        <div className="text-center">
          <p>{error instanceof Error ? error.message : "Unknown error"}</p>
          <Button onClick={() => refetch()} variant="outline" className="mt-2">
            Retry
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div>
      <div className="rounded-md border">
        <Table containerClassName={containerClassName} className={tableClassName}>
          <TableHeader>
            {table.getHeaderGroups().map((headerGroup) => (
              <TableRow key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <TableHead key={header.id}>
                    {header.isPlaceholder
                      ? null
                      : flexRender(header.column.columnDef.header, header.getContext())}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody>
            {table.getRowModel().rows.length ? (
              table.getRowModel().rows.map((row: Row<TData>) => (
                <TableRow key={row.id} data-state={row.getIsSelected() && "selected"}>
                  {row.getVisibleCells().map((cell) => (
                    <TableCell key={cell.id}>
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : (
              <TableRow>
                <TableCell colSpan={columns.length} className="h-24 text-center text-muted-foreground">
                  {emptyMessage}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      <div className="flex items-center justify-end gap-2 py-3">
        <span className="flex-1 text-xs text-muted-foreground">
          Page {pagination.pageIndex + 1} of {pageCount}
        </span>
        <Button
          variant="outline"
          size="sm"
          onClick={() => table.previousPage()}
          disabled={!data?.pagination.has_previous}
        >
          Previous
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => table.nextPage()}
          disabled={!data?.pagination.has_next}
        >
          Next
        </Button>
      </div>
    </div>
  )
}

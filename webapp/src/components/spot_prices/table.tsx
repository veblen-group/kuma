"use client"

import * as React from "react"
import { DataTable } from "@/components/ui/data-table"
import { columns } from "./columns"
import { useSpotPrices } from "@/lib/api-client"
import { useStrategy } from "@/components/strategy-provider"

export function SpotPriceTable() {
  const { pair } = useStrategy()
  const [pagination, setPagination] = React.useState({ pageIndex: 0, pageSize: 5 })

  const { data, isLoading, isError, error, refetch } = useSpotPrices(
    { page: pagination.pageIndex + 1, pageSize: pagination.pageSize },
    { placeholderData: (prev) => prev, staleTime: 30_000, refetchInterval: 30_000 },
  )

  return (
    <DataTable
      columns={columns}
      data={data}
      isLoading={isLoading}
      isError={isError}
      error={error instanceof Error ? error : null}
      refetch={refetch}
      pagination={pagination}
      onPaginationChange={setPagination}
      emptyMessage={`No spot prices for ${pair}.`}
      loadingMessage="Loading spot prices..."
      containerClassName="overflow-visible"
      tableClassName="[&_th]:h-8 [&_th]:px-2 [&_th]:text-xs [&_td]:py-1.5 [&_td]:px-2 [&_td]:text-xs"
    />
  )
}

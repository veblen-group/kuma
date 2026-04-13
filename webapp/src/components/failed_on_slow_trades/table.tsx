"use client"

import * as React from "react"
import { DataTable } from "@/components/ui/data-table"
import { columns } from "./columns"
import { useFailedOnSlowTradeResults } from "@/lib/api-client"
import { useStrategy } from "@/components/strategy-provider"

export function FailedOnSlowTradeResultTable() {
  const { pair } = useStrategy()
  const [pagination, setPagination] = React.useState({ pageIndex: 0, pageSize: 10 })

  const { data, isLoading, isError, error, refetch } = useFailedOnSlowTradeResults(
    { page: pagination.pageIndex + 1, pageSize: pagination.pageSize },
    { placeholderData: (prev) => prev, staleTime: 1000 * 60 * 5 },
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
      emptyMessage={`No failed trades for ${pair}.`}
      loadingMessage="Loading trade results..."
    />
  )
}

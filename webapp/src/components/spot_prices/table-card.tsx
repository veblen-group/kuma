'use client'

import { useRefreshSpotPrices, useSpotPrices } from "@/lib/api-client"
import { SpotPriceTable } from "./table"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { RefreshCw } from "lucide-react"

export function SpotPriceTableCard() {
  const refresh = useRefreshSpotPrices()
  // isFetching from any active spot prices query — use page 1 as a proxy
  const { isFetching } = useSpotPrices({ page: 1, pageSize: 5 })

  return (
    <Card className="flex-1">
      <CardContent className="px-4 py-3">
        <div className="flex items-center justify-between mb-2">
          <p className="text-sm font-semibold">Spot Prices</p>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => refresh()}
            disabled={isFetching}
            className="h-7 w-7 p-0"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isFetching ? "animate-spin" : ""}`} />
          </Button>
        </div>
        <SpotPriceTable />
      </CardContent>
    </Card>
  )
}

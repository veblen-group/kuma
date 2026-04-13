'use client'

import { useSpotPricesChart } from "@/lib/api-client"
import { SpotPriceChart } from "./chart"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { RefreshCw } from "lucide-react"

export function SpotPriceChartCard() {
  const { refetch, isFetching } = useSpotPricesChart()

  return (
    <Card>
      <CardHeader className="py-3 px-4">
        <div className="flex items-center justify-between">
          <CardTitle>Price Chart</CardTitle>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => refetch()}
            disabled={isFetching}
            className="h-7 w-7 p-0"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isFetching ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="px-4 pb-4">
        <div className="h-[480px]">
          <SpotPriceChart />
        </div>
      </CardContent>
    </Card>
  )
}

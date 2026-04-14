'use client'

import { useRefreshSpotPrices, useSpotPrices } from "@/lib/api-client"
import { SpotPriceTable } from "./table"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { MoveRight, RefreshCw } from "lucide-react"
import { useStrategy } from "@/components/strategy-provider"
import { TokenBadge } from "@/components/ui/token-badge"

export function SpotPriceTableCard() {
  const refresh = useRefreshSpotPrices()
  // isFetching from any active spot prices query — use page 1 as a proxy
  const { isFetching } = useSpotPrices({ page: 1, pageSize: 5 })
  const { strategy } = useStrategy()
  // Mirrors Rust's token_a_b_adjusted_for_usdc() + ProtocolSim::spot_price(base, quote):
  // USDC is always the quote (price unit), so prices are expressed as "USDC per non-USDC".
  // e.g. USDC/WETH strategy → base=WETH, quote=USDC → price ≈ 3000 (not ~0.000333).
  const priceFrom = strategy.tokenA === 'USDC' ? strategy.tokenB : strategy.tokenA
  const priceTo   = strategy.tokenA === 'USDC' ? strategy.tokenA : strategy.tokenB

  return (
    <Card className="flex-1">
      <CardContent className="px-4 py-3">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-3">
            <p className="text-sm font-semibold">Spot Prices</p>
            <div className="flex items-center gap-1.5">
              <TokenBadge symbol={priceFrom} />
              <MoveRight size={12} className="shrink-0 text-muted-foreground" />
              <TokenBadge symbol={priceTo} />
            </div>
          </div>
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

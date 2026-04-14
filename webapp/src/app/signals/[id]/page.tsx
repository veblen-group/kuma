"use client"

import { use } from "react"
import Link from "next/link"
import { ArrowLeft, MoveRight } from "lucide-react"
import { useSignal } from "@/lib/api-client"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { ChainBadge } from "@/components/ui/chain-badge"
import { TokenBadge } from "@/components/ui/token-badge"
import { TokenAmount } from "@/components/ui/token-amount"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { BlockCell } from "@/components/ui/block-cell"
import { HoverPopover } from "@/components/ui/hover-popover"
import { Button } from "@/components/ui/button"
import { SpotPrice } from "@/lib/types"

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between py-1 border-b last:border-0">
      <span className="text-xs text-muted-foreground">{label}</span>
      <div className="text-xs">{children}</div>
    </div>
  )
}

function TokenPairLabel({ tokenA, tokenB, logoSize = 16 }: { tokenA: string; tokenB: string; logoSize?: number }) {
  return (
    <div className="flex items-center gap-1.5">
      <TokenBadge symbol={tokenA} size={logoSize} />
      <MoveRight size={12} className="shrink-0 text-muted-foreground" />
      <TokenBadge symbol={tokenB} size={logoSize} />
    </div>
  )
}

function SpotPriceRow({ tokenA, tokenB, price }: { tokenA: string; tokenB: string; price: SpotPrice | null }) {
  if (!price) return null
  return (
    <div className="flex items-center justify-between py-1 border-b last:border-0">
      <TokenPairLabel tokenA={tokenA} tokenB={tokenB} />
      <div className="flex items-center gap-4 tabular-nums font-mono text-xs">
        <span><span className="text-muted-foreground">min </span>{price.min_price.toFixed(6)}</span>
        <span><span className="text-muted-foreground">max </span>{price.max_price.toFixed(6)}</span>
      </div>
    </div>
  )
}

export default function SignalPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = use(params)
  const signalId = Number(id)

  const { data: signal, isLoading, isError, error, refetch } = useSignal(signalId, {
    staleTime: 1000 * 60 * 5,
  })

  if (isLoading) {
    return (
      <main className="container mx-auto px-4 py-6">
        <div className="flex items-center justify-center h-48 text-muted-foreground">
          Loading signal...
        </div>
      </main>
    )
  }

  if (isError || !signal) {
    return (
      <main className="container mx-auto px-4 py-6 space-y-4">
        <Link href="/" className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground">
          <ArrowLeft className="h-4 w-4" /> Back
        </Link>
        <div className="flex flex-col items-center justify-center h-48 gap-4">
          <p className="text-red-500">
            {error instanceof Error ? error.message : "Signal not found"}
          </p>
          <Button variant="outline" size="sm" onClick={() => refetch()}>Retry</Button>
        </div>
      </main>
    )
  }

  const ep = signal.expected_profit

  return (
    <main className="container mx-auto px-4 py-6 space-y-4">
      <div className="space-y-1">
        <Link href="/" className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground">
          <ArrowLeft className="h-4 w-4" /> Back
        </Link>
        <h1 className="text-2xl font-bold text-primary">Signal #{signalId}</h1>
      </div>

      {/* Summary card */}
      <Card className="py-5">
        <CardContent>
          <div className="flex flex-wrap items-center gap-8">
            <div className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">Pair</span>
              <TokenPairLabel tokenA={ep.token_a} tokenB={ep.token_b} logoSize={28} />
            </div>
            <div className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">Slow chain</span>
              <div className="flex items-center gap-1.5">
                <ChainBadge chain={signal.slow.chain} size={28} />
                <BlockCell chain={signal.slow.chain} height={signal.slow.height} timestamp={signal.slow_prices_a_b?.created_at} />
              </div>
            </div>
            <div className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">Fast chain</span>
              <div className="flex items-center gap-1.5">
                <ChainBadge chain={signal.fast.chain} size={28} />
                <BlockCell chain={signal.fast.chain} height={signal.fast.height} timestamp={signal.fast_prices_a_b_created_at} />
              </div>
            </div>
            <div className="flex flex-col gap-1 ml-auto">
              <span className="text-xs text-muted-foreground">Minimum Expected Profit</span>
              <TokenAmount amount={ep.min_total_amount_usdc} symbol="USDC" />
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Swap legs */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm flex items-center gap-2">
              <ChainBadge chain={signal.slow.chain} size={20} /> Simulation Results
            </CardTitle>
          </CardHeader>
          <CardContent>
            <Row label="Block">
              <BlockCell chain={signal.slow.chain} height={signal.slow.height} timestamp={signal.slow_prices_a_b?.created_at} />
            </Row>
            <Row label="Pool">
              <ExplorerLink chain={signal.slow.chain} type="address" value={signal.slow.pool_id} />
            </Row>
            <Row label="Amount In">
              <TokenAmount amount={signal.slow.amount_in} symbol={signal.slow.token_in} />
            </Row>
            <Row label="Amount Out">
              <TokenAmount amount={signal.slow.amount_out} symbol={signal.slow.token_out} />
            </Row>
            <Row label="Gas Cost">
              <TokenAmount amount={signal.slow.gas_cost} symbol="ETH" />
            </Row>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm flex items-center gap-2">
              <ChainBadge chain={signal.fast.chain} size={20} /> Simulation Results
            </CardTitle>
          </CardHeader>
          <CardContent>
            <Row label="Block">
              <BlockCell chain={signal.fast.chain} height={signal.fast.height} timestamp={signal.fast_prices_a_b_created_at} />
            </Row>
            <Row label="Pool">
              <ExplorerLink chain={signal.fast.chain} type="address" value={signal.fast.pool_id} />
            </Row>
            <Row label="Amount In">
              <TokenAmount amount={signal.fast.amount_in} symbol={signal.fast.token_in} />
            </Row>
            <Row label="Amount Out">
              <TokenAmount amount={signal.fast.amount_out} symbol={signal.fast.token_out} />
            </Row>
            <Row label="Gas Cost">
              <TokenAmount amount={signal.fast.gas_cost} symbol="ETH" />
            </Row>
          </CardContent>
        </Card>
      </div>

      {/* Profit & Gas — 3-column layout */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle>Profit &amp; Gas</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-4 lg:gap-x-8">
            <div>
              <p className="text-xs text-muted-foreground font-medium mb-1">Surplus</p>
              <Row label={`Surplus ${ep.token_a}`}>
                <TokenAmount amount={ep.surplus_a} symbol={ep.token_a} />
              </Row>
              <Row label={`Min USDC (${ep.token_a})`}>
                <TokenAmount amount={ep.min_usdc_amount_a} symbol="USDC" />
              </Row>
              <Row label={`Surplus ${ep.token_b}`}>
                <TokenAmount amount={ep.surplus_b} symbol={ep.token_b} />
              </Row>
              <Row label={`Min USDC (${ep.token_b})`}>
                <TokenAmount amount={ep.min_usdc_amount_b} symbol="USDC" />
              </Row>
              <Row label="Min Expected Profit">
                <TokenAmount amount={ep.min_total_amount_usdc} symbol="USDC" />
              </Row>
            </div>
            <div>
              <p className="text-xs text-muted-foreground font-medium mb-1">Gas</p>
              <Row label="Slow">
                <HoverPopover content={<span className="text-xs"><TokenAmount amount={ep.gas_cost_usdc_slow} symbol="USDC" /></span>}>
                  <span className="tabular-nums font-mono">{ep.gas_cost_eth_slow} <span className="text-muted-foreground text-xs">wei</span></span>
                </HoverPopover>
              </Row>
              <Row label="Fast">
                <HoverPopover content={<span className="text-xs"><TokenAmount amount={ep.gas_cost_usdc_fast} symbol="USDC" /></span>}>
                  <span className="tabular-nums font-mono">{ep.gas_cost_eth_fast} <span className="text-muted-foreground text-xs">wei</span></span>
                </HoverPopover>
              </Row>
              <Row label="Total">
                <HoverPopover content={<span className="text-xs"><TokenAmount amount={ep.total_gas_cost_usdc} symbol="USDC" /></span>}>
                  <span className="tabular-nums font-mono">{ep.total_gas_cost_eth} <span className="text-muted-foreground text-xs">wei</span></span>
                </HoverPopover>
              </Row>
            </div>
            <div>
              <p className="text-xs text-muted-foreground font-medium mb-1">Config</p>
              <Row label="Max Slippage">
                <span className="tabular-nums font-mono">{signal.max_slippage_bps} bps</span>
              </Row>
              <Row label="Congestion Discount">
                <span className="tabular-nums font-mono">{signal.congestion_risk_discount_bps} bps</span>
              </Row>
              <p className="text-xs text-muted-foreground font-medium mt-3 mb-1">Prices Used</p>
              <Row label={`${ep.token_a}/USDC`}>
                <span className="tabular-nums font-mono">{ep.token_usdc_price_a.toFixed(6)}</span>
              </Row>
              <Row label={`${ep.token_b}/USDC`}>
                <span className="tabular-nums font-mono">{ep.token_usdc_price_b.toFixed(6)}</span>
              </Row>
              <Row label="ETH/USDC">
                <span className="tabular-nums font-mono">{ep.eth_usdc_price.toFixed(2)}</span>
              </Row>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Spot Prices */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm flex items-center gap-2">
            Spot Prices at Signal Time <ChainBadge chain={signal.slow.chain} />
          </CardTitle>
        </CardHeader>
        <CardContent>
          <SpotPriceRow tokenA={ep.token_a} tokenB={ep.token_b} price={signal.slow_prices_a_b} />
          <SpotPriceRow tokenA={ep.token_a} tokenB="USDC" price={signal.slow_prices_a_usdc} />
          <SpotPriceRow tokenA={ep.token_b} tokenB="USDC" price={signal.slow_prices_b_usdc} />
          <SpotPriceRow tokenA="ETH" tokenB="USDC" price={signal.slow_prices_eth_usdc} />
        </CardContent>
      </Card>
    </main>
  )
}

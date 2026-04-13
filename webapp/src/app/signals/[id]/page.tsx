"use client"

import { use } from "react"
import Link from "next/link"
import { ArrowLeft } from "lucide-react"
import { useSignal } from "@/lib/api-client"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { ChainBadge } from "@/components/ui/chain-badge"
import { TokenBadge } from "@/components/ui/token-badge"
import { TokenAmount } from "@/components/ui/token-amount"
import { ExplorerLink } from "@/components/ui/explorer-link"
import { BlockCell } from "@/components/ui/block-cell"
import { Button } from "@/components/ui/button"
import { SpotPrice } from "@/lib/types"

function LabeledRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between py-1.5 border-b last:border-0">
      <span className="text-sm text-muted-foreground">{label}</span>
      <div className="text-sm">{children}</div>
    </div>
  )
}

function SpotPriceRow({ label, price }: { label: string; price: SpotPrice | null }) {
  if (!price) return null
  return (
    <LabeledRow label={label}>
      <span className="tabular-nums font-mono text-xs">
        {price.min_price.toFixed(6)} – {price.max_price.toFixed(6)}
      </span>
    </LabeledRow>
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
      <main className="container mx-auto px-4 py-8">
        <div className="flex items-center justify-center h-48 text-muted-foreground">
          Loading signal...
        </div>
      </main>
    )
  }

  if (isError || !signal) {
    return (
      <main className="container mx-auto px-4 py-8 space-y-4">
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

  return (
    <main className="container mx-auto px-4 py-8 space-y-6">
      <div className="space-y-1">
        <Link href="/" className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground">
          <ArrowLeft className="h-4 w-4" /> Back
        </Link>
        <h1 className="text-2xl font-bold text-primary">Signal #{signalId}</h1>
      </div>

      {/* Swap legs */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ChainBadge chain={signal.slow.chain} /> Slow Chain Swap
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-0">
            <LabeledRow label="Block">
              <BlockCell chain={signal.slow.chain} height={signal.slow.height} timestamp={signal.slow_prices_a_b?.created_at} />
            </LabeledRow>
            <LabeledRow label="Pool">
              <ExplorerLink chain={signal.slow.chain} type="address" value={signal.slow.pool_id} />
            </LabeledRow>
            <LabeledRow label="Token In">
              <TokenBadge symbol={signal.slow.token_in} />
            </LabeledRow>
            <LabeledRow label="Token Out">
              <TokenBadge symbol={signal.slow.token_out} />
            </LabeledRow>
            <LabeledRow label="Amount In">
              <TokenAmount amount={signal.slow.amount_in} symbol={signal.slow.token_in} />
            </LabeledRow>
            <LabeledRow label="Amount Out">
              <TokenAmount amount={signal.slow.amount_out} symbol={signal.slow.token_out} />
            </LabeledRow>
            <LabeledRow label="Gas Cost">
              <TokenAmount amount={signal.slow.gas_cost} symbol="ETH" />
            </LabeledRow>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ChainBadge chain={signal.fast.chain} /> Fast Chain Swap
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-0">
            <LabeledRow label="Block">
              <BlockCell chain={signal.fast.chain} height={signal.fast.height} />
            </LabeledRow>
            <LabeledRow label="Pool">
              <ExplorerLink chain={signal.fast.chain} type="address" value={signal.fast.pool_id} />
            </LabeledRow>
            <LabeledRow label="Token In">
              <TokenBadge symbol={signal.fast.token_in} />
            </LabeledRow>
            <LabeledRow label="Token Out">
              <TokenBadge symbol={signal.fast.token_out} />
            </LabeledRow>
            <LabeledRow label="Amount In">
              <TokenAmount amount={signal.fast.amount_in} symbol={signal.fast.token_in} />
            </LabeledRow>
            <LabeledRow label="Amount Out">
              <TokenAmount amount={signal.fast.amount_out} symbol={signal.fast.token_out} />
            </LabeledRow>
            <LabeledRow label="Gas Cost">
              <TokenAmount amount={signal.fast.gas_cost} symbol="ETH" />
            </LabeledRow>
          </CardContent>
        </Card>
      </div>

      {/* Expected Profit */}
      <Card>
        <CardHeader>
          <CardTitle>Expected Profit</CardTitle>
        </CardHeader>
        <CardContent className="space-y-0">
          <LabeledRow label="Surplus A">
            <TokenAmount amount={signal.expected_profit.surplus_a} symbol={signal.expected_profit.token_a} />
          </LabeledRow>
          <LabeledRow label="Surplus B">
            <TokenAmount amount={signal.expected_profit.surplus_b} symbol={signal.expected_profit.token_b} />
          </LabeledRow>
          <LabeledRow label="Gas Cost (Slow)">
            <TokenAmount amount={signal.expected_profit.gas_cost_usdc_slow} symbol="USDC" />
          </LabeledRow>
          <LabeledRow label="Gas Cost (Fast)">
            <TokenAmount amount={signal.expected_profit.gas_cost_usdc_fast} symbol="USDC" />
          </LabeledRow>
          <LabeledRow label="Total Gas Cost">
            <TokenAmount amount={signal.expected_profit.total_gas_cost_usdc} symbol="USDC" />
          </LabeledRow>
          <LabeledRow label="Min Profit">
            <TokenAmount amount={signal.expected_profit.min_total_amount_usdc} symbol="USDC" />
          </LabeledRow>
          <LabeledRow label="Max Slippage">
            <span className="tabular-nums font-mono text-xs">{signal.max_slippage_bps} bps</span>
          </LabeledRow>
          <LabeledRow label="Congestion Discount">
            <span className="tabular-nums font-mono text-xs">{signal.congestion_risk_discount_bps} bps</span>
          </LabeledRow>
        </CardContent>
      </Card>

      {/* Spot Prices */}
      <Card>
        <CardHeader>
          <CardTitle>Spot Prices at Signal Time</CardTitle>
        </CardHeader>
        <CardContent className="space-y-0">
          <SpotPriceRow
            label={`${signal.expected_profit.token_a}/${signal.expected_profit.token_b}`}
            price={signal.slow_prices_a_b}
          />
          <SpotPriceRow
            label={`${signal.expected_profit.token_a}/USDC`}
            price={signal.slow_prices_a_usdc}
          />
          <SpotPriceRow
            label={`${signal.expected_profit.token_b}/USDC`}
            price={signal.slow_prices_b_usdc}
          />
          <SpotPriceRow
            label="ETH/USDC"
            price={signal.slow_prices_eth_usdc}
          />
        </CardContent>
      </Card>
    </main>
  )
}

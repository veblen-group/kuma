'use client'

import { TOKEN_INFO, STRATEGY } from '@/lib/token-config'
import { useTokenPrices } from '@/lib/use-token-prices'
import { useTokenList } from '@/lib/use-token-list'
import { ChainBadge } from '@/components/ui/chain-badge'
import React from 'react'

interface TokenDisplayProps {
  symbol: string
  price: number | undefined
  logoUrl: string | undefined
}

function TokenDisplay({ symbol, price, logoUrl }: TokenDisplayProps) {
  const [imgError, setImgError] = React.useState(false)

  return (
    <div className="flex flex-col items-center gap-1">
      {logoUrl && !imgError ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={logoUrl}
          alt={symbol}
          width={28}
          height={28}
          className="rounded-full shrink-0"
          onError={() => setImgError(true)}
        />
      ) : (
        <span className="w-7 h-7 rounded-full bg-muted flex items-center justify-center text-[9px] font-bold text-muted-foreground shrink-0">
          {symbol.slice(0, 2).toUpperCase()}
        </span>
      )}
      <span className="font-semibold">{symbol}</span>
      <span className="text-xs text-muted-foreground">
        {price !== undefined
          ? price.toLocaleString('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 2 })
          : '—'}
      </span>
    </div>
  )
}

export function StrategyCard() {
  const tokenList = useTokenList()
  const tokenA = TOKEN_INFO[STRATEGY.token_a]
  const tokenB = TOKEN_INFO[STRATEGY.token_b]

  const prices = useTokenPrices([
    tokenA?.ethereumAddress ?? null,
    tokenB?.ethereumAddress ?? null,
  ])

  const logoA = tokenA?.ethereumAddress ? tokenList.get(tokenA.ethereumAddress) : undefined
  const logoB = tokenB?.ethereumAddress ? tokenList.get(tokenB.ethereumAddress) : undefined

  const priceA = tokenA?.ethereumAddress ? prices[tokenA.ethereumAddress] : undefined
  const priceB = tokenB?.ethereumAddress ? prices[tokenB.ethereumAddress] : undefined

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-4">
        <div className="rounded-md border px-3 py-2 flex flex-col items-center">
          <p className="text-xs text-muted-foreground self-start mb-1">Token A</p>
          <TokenDisplay symbol={STRATEGY.token_a} price={priceA} logoUrl={logoA} />
        </div>
        <div className="rounded-md border px-3 py-2 flex flex-col items-center">
          <p className="text-xs text-muted-foreground self-start mb-1">Token B</p>
          <TokenDisplay symbol={STRATEGY.token_b} price={priceB} logoUrl={logoB} />
        </div>
      </div>

      <div className="rounded-md border px-3 py-2 space-y-1">
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Slow chain</span>
          <ChainBadge chain={STRATEGY.slow_chain} />
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Fast chain</span>
          <ChainBadge chain={STRATEGY.fast_chain} />
        </div>
      </div>
    </div>
  )
}

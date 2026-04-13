'use client'

import { TOKEN_INFO } from '@/lib/token-config'
import { useStrategy, TOKEN_NAMES, CHAIN_NAMES } from '@/components/strategy-provider'
import { useTokenPrices } from '@/lib/use-token-prices'
import { useTokenList } from '@/lib/use-token-list'
import { getChainName, getChainLogoUrl } from '@/lib/chains'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import React from 'react'

function TokenLogo({ symbol, logoUrl }: { symbol: string; logoUrl: string | undefined }) {
  const [imgError, setImgError] = React.useState(false)

  if (logoUrl && !imgError) {
    return (
      // eslint-disable-next-line @next/next/no-img-element
      <img
        src={logoUrl}
        alt={symbol}
        width={36}
        height={36}
        className="rounded-full shrink-0"
        onError={() => setImgError(true)}
      />
    )
  }

  return (
    <span className="w-9 h-9 rounded-full bg-muted flex items-center justify-center text-[10px] font-bold text-muted-foreground shrink-0">
      {symbol.slice(0, 2).toUpperCase()}
    </span>
  )
}

function ChainOption({ chain }: { chain: string }) {
  const logoUrl = getChainLogoUrl(chain)
  const name = getChainName(chain)
  const [imgError, setImgError] = React.useState(false)

  return (
    <span className="inline-flex items-center gap-2">
      {logoUrl && !imgError ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={logoUrl}
          alt={name}
          width={18}
          height={18}
          className="rounded-full shrink-0"
          onError={() => setImgError(true)}
        />
      ) : (
        <span className="w-[18px] h-[18px] rounded-full bg-muted flex items-center justify-center text-[8px] font-bold text-muted-foreground shrink-0">
          {name.slice(0, 2).toUpperCase()}
        </span>
      )}
      <span>{name}</span>
    </span>
  )
}

export function StrategyCard() {
  const { strategy, setStrategy } = useStrategy()
  const tokenList = useTokenList()

  const tokenA = TOKEN_INFO[strategy.tokenA]
  const tokenB = TOKEN_INFO[strategy.tokenB]

  const prices = useTokenPrices([
    tokenA?.ethereumAddress ?? null,
    tokenB?.ethereumAddress ?? null,
  ])

  const logoA = tokenA?.ethereumAddress ? tokenList.get(tokenA.ethereumAddress) : undefined
  const logoB = tokenB?.ethereumAddress ? tokenList.get(tokenB.ethereumAddress) : undefined

  const priceA = tokenA?.ethereumAddress ? prices[tokenA.ethereumAddress] : undefined
  const priceB = tokenB?.ethereumAddress ? prices[tokenB.ethereumAddress] : undefined

  const formatPrice = (price: number | undefined) =>
    price !== undefined
      ? price.toLocaleString('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 2 })
      : '—'

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-4">
        <div className="flex flex-col items-center gap-1.5">
          <p className="text-xs text-muted-foreground">Token A</p>
          <TokenLogo symbol={strategy.tokenA} logoUrl={logoA} />
          <Select
            value={strategy.tokenA}
            onValueChange={(v) => setStrategy({ ...strategy, tokenA: v })}
          >
            <SelectTrigger className="h-8 w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TOKEN_NAMES.map((t) => (
                <SelectItem key={t} value={t}>{t}</SelectItem>
              ))}
            </SelectContent>
          </Select>
          <span className="text-xs text-muted-foreground">{formatPrice(priceA)}</span>
        </div>
        <div className="flex flex-col items-center gap-1.5">
          <p className="text-xs text-muted-foreground">Token B</p>
          <TokenLogo symbol={strategy.tokenB} logoUrl={logoB} />
          <Select
            value={strategy.tokenB}
            onValueChange={(v) => setStrategy({ ...strategy, tokenB: v })}
          >
            <SelectTrigger className="h-8 w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TOKEN_NAMES.map((t) => (
                <SelectItem key={t} value={t}>{t}</SelectItem>
              ))}
            </SelectContent>
          </Select>
          <span className="text-xs text-muted-foreground">{formatPrice(priceB)}</span>
        </div>
      </div>

      <div className="rounded-md border px-3 py-2.5 space-y-2">
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Slow chain</span>
          <Select
            value={strategy.slowChain}
            onValueChange={(v) => setStrategy({ ...strategy, slowChain: v })}
          >
            <SelectTrigger className="h-8 w-[150px] text-sm">
              <SelectValue>
                <ChainOption chain={strategy.slowChain} />
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              {CHAIN_NAMES.map((c) => (
                <SelectItem key={c} value={c}>
                  <ChainOption chain={c} />
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Fast chain</span>
          <Select
            value={strategy.fastChain}
            onValueChange={(v) => setStrategy({ ...strategy, fastChain: v })}
          >
            <SelectTrigger className="h-8 w-[150px] text-sm">
              <SelectValue>
                <ChainOption chain={strategy.fastChain} />
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              {CHAIN_NAMES.map((c) => (
                <SelectItem key={c} value={c}>
                  <ChainOption chain={c} />
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
    </div>
  )
}

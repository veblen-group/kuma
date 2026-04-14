'use client'

import { TOKEN_INFO } from '@/lib/token-config'
import { useStrategy, TOKEN_NAMES, CHAIN_NAMES } from '@/components/strategy-provider'
import config from '@/generated/kuma.config.json'
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
      <img src={logoUrl} alt={symbol} width={36} height={36}
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
    <span className="inline-flex items-center gap-1.5">
      {logoUrl && !imgError ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img src={logoUrl} alt={name} width={18} height={18}
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

  // Pre-compute logo URLs for all tokens so we can show them in the dropdown
  const tokenLogoMap = React.useMemo(() => {
    const map: Record<string, string | undefined> = {}
    for (const symbol of TOKEN_NAMES) {
      const info = TOKEN_INFO[symbol]
      map[symbol] = info?.ethereumAddress ? tokenList.get(info.ethereumAddress) : undefined
    }
    return map
  }, [tokenList])

  const priceA = tokenA?.ethereumAddress ? prices[tokenA.ethereumAddress] : undefined
  const priceB = tokenB?.ethereumAddress ? prices[tokenB.ethereumAddress] : undefined

  const fmtPrice = (p: number | undefined) =>
    p !== undefined
      ? p.toLocaleString('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 2 })
      : '—'

  return (
    <div className="space-y-3">
      {/* Token selectors */}
      <div className="grid grid-cols-2 gap-3">
        <div className="flex flex-col items-center gap-1">
          <p className="text-[10px] text-muted-foreground">Token A</p>
          <Select value={strategy.tokenA} onValueChange={(v) => setStrategy({ ...strategy, tokenA: v })}>
            <SelectTrigger className="h-auto w-auto border-0 shadow-none px-1.5 py-0.5 gap-0.5 font-semibold text-sm focus:ring-0 rounded-md hover:bg-muted transition-colors [&>span]:line-clamp-none">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TOKEN_NAMES.map((t) => (
                <SelectItem key={t} value={t}>
                  <span className="flex items-center gap-2">
                    <TokenLogo symbol={t} logoUrl={tokenLogoMap[t]} />
                    <span>{t}</span>
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <span className="text-xs text-muted-foreground">{fmtPrice(priceA)}</span>
        </div>
        <div className="flex flex-col items-center gap-1">
          <p className="text-[10px] text-muted-foreground">Token B</p>
          <Select value={strategy.tokenB} onValueChange={(v) => setStrategy({ ...strategy, tokenB: v })}>
            <SelectTrigger className="h-auto w-auto border-0 shadow-none px-1.5 py-0.5 gap-0.5 font-semibold text-sm focus:ring-0 rounded-md hover:bg-muted transition-colors [&>span]:line-clamp-none">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TOKEN_NAMES.map((t) => (
                <SelectItem key={t} value={t}>
                  <span className="flex items-center gap-2">
                    <TokenLogo symbol={t} logoUrl={tokenLogoMap[t]} />
                    <span>{t}</span>
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <span className="text-xs text-muted-foreground">{fmtPrice(priceB)}</span>
        </div>
      </div>

      {/* Chain selectors + inventory */}
      <div className="border-t pt-2 text-xs">
        <div className="grid grid-cols-2 gap-x-4 gap-y-1">
          {/* Column headers */}
          <p className="text-[10px] text-muted-foreground text-center">Slow chain</p>
          <p className="text-[10px] text-muted-foreground text-center">Fast chain</p>

          {/* Selectors */}
          <div className="flex items-center justify-center">
            <Select value={strategy.slowChain} onValueChange={(v) => setStrategy({ ...strategy, slowChain: v })}>
              <SelectTrigger className="h-auto w-auto border-0 shadow-none px-1.5 py-0.5 gap-0.5 text-xs font-medium focus:ring-0 rounded-md hover:bg-muted transition-colors [&>span]:line-clamp-none">
                <SelectValue><ChainOption chain={strategy.slowChain} /></SelectValue>
              </SelectTrigger>
              <SelectContent>
                {CHAIN_NAMES.map((c) => <SelectItem key={c} value={c}><ChainOption chain={c} /></SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <div className="flex items-center justify-center">
            <Select value={strategy.fastChain} onValueChange={(v) => setStrategy({ ...strategy, fastChain: v })}>
              <SelectTrigger className="h-auto w-auto border-0 shadow-none px-1.5 py-0.5 gap-0.5 text-xs font-medium focus:ring-0 rounded-md hover:bg-muted transition-colors [&>span]:line-clamp-none">
                <SelectValue><ChainOption chain={strategy.fastChain} /></SelectValue>
              </SelectTrigger>
              <SelectContent>
                {CHAIN_NAMES.map((c) => <SelectItem key={c} value={c}><ChainOption chain={c} /></SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          {/* "Inventory" label spanning both columns */}
          <div className="col-span-2 text-center">
            <span className="text-[10px] text-muted-foreground">Inventory</span>
          </div>

          {/* Inventory rows */}
          {([
            { symbol: strategy.tokenA, logo: tokenLogoMap[strategy.tokenA] },
            { symbol: strategy.tokenB, logo: tokenLogoMap[strategy.tokenB] },
          ] as { symbol: TokenKey; logo: string | undefined }[]).map(({ symbol, logo }) => {
            const tokenCfg = config.tokens[symbol]
            if (!tokenCfg) return null
            return (
              <React.Fragment key={symbol}>
                {[strategy.slowChain, strategy.fastChain].map(chain => {
                  const inv = (tokenCfg.inventory as Record<string, string>)[chain]
                  return (
                    <div key={chain} className="flex items-center justify-center gap-1.5">
                      <span className="tabular-nums font-medium">{inv ? fmtInventory(inv, tokenCfg.decimals) : '—'}</span>
                      <span className="inline-flex items-center gap-1 text-muted-foreground">
                        {logo
                          ? <img src={logo} alt={symbol} width={14} height={14} className="rounded-full shrink-0" />
                          : <span className="w-3.5 h-3.5 rounded-full bg-muted shrink-0" />
                        }
                        {symbol}
                      </span>
                    </div>
                  )
                })}
              </React.Fragment>
            )
          })}

        </div>
      </div>
    </div>
  )
}

type TokenKey = keyof typeof config.tokens

function fmtInventory(raw: string, decimals: number): string {
  const value = Number(BigInt(raw)) / Math.pow(10, decimals)
  return value.toLocaleString('en-US', { maximumFractionDigits: 4 })
}

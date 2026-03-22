'use client'

import { TokenBadge } from '@/components/ui/token-badge'
import { formatTokenAmount, formatWithDecimals, TOKEN_INFO } from '@/lib/token-config'

interface TokenAmountProps {
  /** Raw on-chain amount string (BigInt denominated) */
  amount: string
  symbol: string
}

export function TokenAmount({ amount, symbol }: TokenAmountProps) {
  const info = TOKEN_INFO[symbol]
  const formatted = info
    ? formatWithDecimals(amount, info.decimals)
    : formatTokenAmount(amount, symbol)

  return (
    <span className="inline-flex items-center gap-1.5 tabular-nums">
      <TokenBadge symbol={symbol} showName={false} />
      <span>{formatted}</span>
      <span className="text-muted-foreground text-xs">{symbol}</span>
    </span>
  )
}

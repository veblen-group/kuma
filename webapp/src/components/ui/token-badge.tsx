'use client'

import React from 'react'
import { TOKEN_INFO } from '@/lib/token-config'
import { useTokenList } from '@/lib/use-token-list'

interface TokenBadgeProps {
  symbol: string
  showName?: boolean
  size?: number
}

export function TokenBadge({ symbol, showName = true, size = 16 }: TokenBadgeProps) {
  const tokenList = useTokenList()
  const info = TOKEN_INFO[symbol]
  const logoUrl = info?.ethereumAddress ? tokenList.get(info.ethereumAddress) : undefined
  const [imgError, setImgError] = React.useState(false)

  return (
    <span className="inline-flex items-center gap-1.5">
      {logoUrl && !imgError ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={logoUrl}
          alt={symbol}
          width={size}
          height={size}
          className="rounded-full shrink-0"
          onError={() => setImgError(true)}
        />
      ) : (
        <span
          className="rounded-full bg-muted flex items-center justify-center font-bold text-muted-foreground shrink-0"
          style={{ width: size, height: size, fontSize: Math.max(8, size * 0.5) }}
        >
          {symbol.slice(0, 2).toUpperCase()}
        </span>
      )}
      {showName && <span className="font-medium text-sm">{symbol}</span>}
    </span>
  )
}

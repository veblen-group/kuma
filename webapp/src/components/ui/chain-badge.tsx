'use client'

import React from 'react'
import { getChainName, getChainLogoUrl } from '@/lib/chains'

interface ChainBadgeProps {
  chain: string
  showName?: boolean
  size?: number
}

export function ChainBadge({ chain, showName = true, size = 16 }: ChainBadgeProps) {
  const name = getChainName(chain)
  const logoUrl = getChainLogoUrl(chain)
  const [imgError, setImgError] = React.useState(false)

  return (
    <span className="inline-flex items-center gap-1.5">
      {logoUrl && !imgError ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={logoUrl}
          alt={name}
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
          {name.slice(0, 2).toUpperCase()}
        </span>
      )}
      {showName && <span className="text-sm">{name}</span>}
    </span>
  )
}

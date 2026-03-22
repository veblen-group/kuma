'use client'

import React from 'react'
import { getChainName, getChainLogoUrl } from '@/lib/chains'

interface ChainBadgeProps {
  chain: string
  showName?: boolean
}

export function ChainBadge({ chain, showName = true }: ChainBadgeProps) {
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
          width={16}
          height={16}
          className="rounded-full shrink-0"
          onError={() => setImgError(true)}
        />
      ) : (
        <span className="w-4 h-4 rounded-full bg-muted flex items-center justify-center text-[8px] font-bold text-muted-foreground shrink-0">
          {name.slice(0, 2).toUpperCase()}
        </span>
      )}
      {showName && <span className="text-sm">{name}</span>}
    </span>
  )
}

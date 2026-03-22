'use client'

import { ExternalLink } from 'lucide-react'
import { getExplorerUrl, type ExplorerType } from '@/lib/chains'

interface ExplorerLinkProps {
  chain: string
  type: ExplorerType
  value: string | number
  display?: string
}

export function ExplorerLink({ chain, type, value, display }: ExplorerLinkProps) {
  const url = getExplorerUrl(chain, type, value)

  const defaultDisplay =
    type === 'block'
      ? `#${value}`
      : typeof value === 'string' && value.startsWith('0x') && value.length > 14
        ? `${value.slice(0, 6)}…${value.slice(-4)}`
        : String(value)

  const text = display ?? defaultDisplay

  if (!url) {
    return <span className="font-mono text-xs text-muted-foreground">{text}</span>
  }

  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      className="font-mono text-xs text-primary hover:underline inline-flex items-center gap-0.5"
      title={String(value)}
    >
      {text}
      <ExternalLink className="h-3 w-3 opacity-50 shrink-0" />
    </a>
  )
}

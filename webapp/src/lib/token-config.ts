import { getAddress } from 'viem'
import config from '@/generated/kuma.config.json'

export interface TokenInfo {
  symbol: string
  decimals: number
  // lowercase ethereum address for tokenlist lookup
  ethereumAddress: string | null
}

type ConfigTokens = typeof config.tokens
type TokenKey = keyof ConfigTokens

export const TOKEN_INFO: Record<string, TokenInfo> = Object.fromEntries(
  (Object.entries(config.tokens) as [TokenKey, ConfigTokens[TokenKey]][]).map(
    ([symbol, info]) => {
      const addresses = info.addresses as Record<string, string>
      let ethereumAddress: string | null = null
      try {
        const raw = addresses['ethereum']
        if (raw) ethereumAddress = getAddress(raw).toLowerCase()
      } catch {
        // ignore invalid addresses
      }
      return [symbol, { symbol, decimals: info.decimals, ethereumAddress }]
    },
  ),
)

export const TOKEN_NAMES = Object.keys(config.tokens)

export function formatWithDecimals(rawAmount: string, decimals: number): string {
  try {
    const raw = BigInt(rawAmount)
    const isNeg = raw < 0n
    const abs = isNeg ? -raw : raw
    const divisor = 10n ** BigInt(decimals)
    const whole = abs / divisor
    const fraction = abs % divisor
    const fractionStr = fraction.toString().padStart(decimals, '0')
    const trimmed = fractionStr.slice(0, 6).replace(/0+$/, '')
    const formatted = trimmed ? `${whole}.${trimmed}` : whole.toString()
    return isNeg ? `-${formatted}` : formatted
  } catch {
    return rawAmount
  }
}

export function formatTokenAmount(rawAmount: string, symbol: string): string {
  const info = TOKEN_INFO[symbol]
  if (!info) return rawAmount
  return formatWithDecimals(rawAmount, info.decimals)
}

export function formatPrice(price: number): string {
  if (price === 0) return '0'
  const abs = Math.abs(price)
  if (abs >= 1000) return price.toLocaleString('en-US', { maximumFractionDigits: 2 })
  if (abs >= 1) return price.toFixed(4)
  if (abs >= 0.0001) return price.toFixed(6)
  return price.toExponential(4)
}

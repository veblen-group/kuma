import { mainnet, base, unichain } from 'viem/chains'
import type { Chain } from 'viem'

const CHAIN_MAP: Record<string, Chain> = {
  ethereum: mainnet,
  base,
  unichain,
}

// DefiLlama chain icon CDN — reliable, no auth required
const CHAIN_LOGO_URLS: Record<string, string> = {
  ethereum: 'https://icons.llamao.fi/icons/chains/rsz_ethereum.jpg',
  base: 'https://icons.llamao.fi/icons/chains/rsz_base.jpg',
  unichain: 'https://icons.llamao.fi/icons/chains/rsz_unichain.jpg',
}

export function getChainName(chain: string): string {
  return CHAIN_MAP[chain]?.name ?? chain
}

export function getChainLogoUrl(chain: string): string | null {
  return CHAIN_LOGO_URLS[chain] ?? null
}

export type ExplorerType = 'tx' | 'block' | 'address'

export function getExplorerUrl(
  chain: string,
  type: ExplorerType,
  value: string | number,
): string | null {
  const baseUrl = CHAIN_MAP[chain]?.blockExplorers?.default?.url
  if (!baseUrl) return null
  return `${baseUrl}/${type}/${value}`
}

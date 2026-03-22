'use client'

import { useQuery } from '@tanstack/react-query'

// EIP-1577 token list standard — using Uniswap's hosted list as the source
const TOKEN_LIST_URL = 'https://tokens.uniswap.org'

interface TokenListToken {
  chainId: number
  address: string
  symbol: string
  logoURI?: string
}

interface TokenList {
  tokens: TokenListToken[]
}

function resolveLogoUri(uri: string): string {
  if (uri.startsWith('ipfs://')) {
    return `https://ipfs.io/ipfs/${uri.slice(7)}`
  }
  return uri
}

// Returns a map of lowercase address → logo URL (ethereum mainnet only)
export function useTokenList(): Map<string, string> {
  const { data } = useQuery<Map<string, string>>({
    queryKey: ['tokenlist'],
    queryFn: async () => {
      const res = await fetch(TOKEN_LIST_URL)
      const list: TokenList = await res.json()
      const map = new Map<string, string>()
      for (const token of list.tokens) {
        if (token.chainId === 1 && token.logoURI) {
          map.set(token.address.toLowerCase(), resolveLogoUri(token.logoURI))
        }
      }
      return map
    },
    staleTime: Infinity,
    gcTime: Infinity,
  })
  return data ?? new Map()
}

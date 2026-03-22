'use client'

import { useQuery } from '@tanstack/react-query'

interface DefiLlamaResponse {
  coins: Record<string, { price: number; symbol: string; confidence: number }>
}

// Returns USD prices keyed by lowercase ethereum address
export function useTokenPrices(ethereumAddresses: (string | null)[]): Record<string, number> {
  const validAddresses = ethereumAddresses.filter((a): a is string => !!a)

  const { data } = useQuery<Record<string, number>>({
    queryKey: ['token-prices', ...validAddresses],
    queryFn: async () => {
      const coins = validAddresses.map((a) => `ethereum:${a}`).join(',')
      const res = await fetch(`https://coins.llama.fi/prices/current/${coins}`)
      const json: DefiLlamaResponse = await res.json()
      const result: Record<string, number> = {}
      for (const [key, val] of Object.entries(json.coins)) {
        const addr = key.split(':')[1]
        if (addr) result[addr.toLowerCase()] = val.price
      }
      return result
    },
    enabled: validAddresses.length > 0,
    staleTime: 60_000,
    refetchInterval: 60_000,
  })

  return data ?? {}
}

"use client"

import * as React from "react"
import config from "@/generated/kuma.config.json"

const defaultStrategy = config.strategies[0]

type ConfigTokens = typeof config.tokens
type TokenKey = keyof ConfigTokens

export const TOKEN_NAMES = Object.keys(config.tokens) as TokenKey[]

// Derive available chain names from token address keys
const firstToken = config.tokens[TOKEN_NAMES[0]]
export const CHAIN_NAMES = Object.keys(
  (firstToken as ConfigTokens[TokenKey]).addresses,
)

export interface StrategySelection {
  tokenA: string
  tokenB: string
  slowChain: string
  fastChain: string
}

interface StrategyContextValue {
  strategy: StrategySelection
  setStrategy: (s: StrategySelection) => void
  pair: string
}

const STORAGE_KEY = "kuma-strategy"

function getDefault(): StrategySelection {
  return {
    tokenA: defaultStrategy.token_a,
    tokenB: defaultStrategy.token_b,
    slowChain: defaultStrategy.slow_chain,
    fastChain: defaultStrategy.fast_chain,
  }
}

function validate(s: StrategySelection): boolean {
  const tokens = TOKEN_NAMES as string[]
  const chains = CHAIN_NAMES as string[]
  return (
    tokens.includes(s.tokenA) &&
    tokens.includes(s.tokenB) &&
    chains.includes(s.slowChain) &&
    chains.includes(s.fastChain)
  )
}

const StrategyContext = React.createContext<StrategyContextValue>({
  strategy: getDefault(),
  setStrategy: () => null,
  pair: `${defaultStrategy.token_a}-${defaultStrategy.token_b}`,
})

export function StrategyProvider({ children }: { children: React.ReactNode }) {
  const [strategy, setStrategyState] =
    React.useState<StrategySelection>(getDefault)

  // Hydrate from localStorage after mount
  React.useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY)
      if (stored) {
        const parsed = JSON.parse(stored) as StrategySelection
        if (validate(parsed)) {
          setStrategyState(parsed)
        }
      }
    } catch {
      // ignore corrupt localStorage
    }
  }, [])

  const setStrategy = React.useCallback((s: StrategySelection) => {
    setStrategyState(s)
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(s))
    } catch {
      // localStorage may be unavailable
    }
  }, [])

  const pair = `${strategy.tokenA}-${strategy.tokenB}`

  const value = React.useMemo(
    () => ({ strategy, setStrategy, pair }),
    [strategy, setStrategy, pair],
  )

  return (
    <StrategyContext.Provider value={value}>
      {children}
    </StrategyContext.Provider>
  )
}

export function useStrategy() {
  const context = React.useContext(StrategyContext)
  if (context === undefined) {
    throw new Error("useStrategy must be used within a StrategyProvider")
  }
  return context
}

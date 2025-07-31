export interface Token {
  symbol: string;
  address: string;
}

export interface Pair {
  token_a: Token;
  token_b: Token;
}

export interface SpotPrice {
  pair: Pair;
  block_height: number;
  price: string;
  pool_id: string;
  chain: string;
}

export interface SwapInfo {
  token_in: Token;
  token_out: Token;
  amount_in: string;
  amount_out: string;
}

export interface ArbitrageSignal {
  block_height: number;
  slow_chain: string;
  slow_pair: Pair;
  slow_pool_id: string;
  fast_chain: string;
  fast_pair: Pair;
  fast_pool_id: string;
  slow_swap: SwapInfo;
  fast_swap: SwapInfo;
  surplus_a: string;
  surplus_b: string;
  expected_profit_a: string;
  expected_profit_b: string;
  max_slippage_bps: number;
}

export interface PaginationInfo {
  page: number;
  page_size: number;
  total_pages?: number;
  total_items?: number;
  has_next: boolean;
  has_previous: boolean;
}

export interface PaginatedResponse<T> {
  data: T[];
  pagination: PaginationInfo;
}
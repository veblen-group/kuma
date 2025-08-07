export interface Token {
  symbol: string;
  address: string;
  decimals: number;
}

export interface Chain {
  name: string;
  rpc_url: string;
  tycho_url: string;
  permit2_address: string;
  metadata: any;
}
export type Pair = [Token, Token];


export interface SpotPrice {
  pair: Pair;
  block_height: number;
  min_price: number;
  max_price: number;
  min_pool_id: string;
  max_pool_id: string;
  chain: Chain;
}

export interface SwapInfo {
  token_in: Token;
  token_out: Token;
  amount_in: string;
  amount_out: string;
}

export interface Signal {
  slow_chain: Chain;
  slow_pair: Pair;
  slow_pool_id: string;
  slow_swap: SwapInfo;
  slow_height: number;
  fast_chain: Chain;
  fast_pair: Pair;
  fast_pool_id: string;
  fast_swap: SwapInfo;
  fast_height: number;
  surplus_a: string;
  surplus_b: string;
  expected_profit_a: string;
  expected_profit_b: string;
  max_slippage_bps: number;
  congestion_risk_discount_bps: number;
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
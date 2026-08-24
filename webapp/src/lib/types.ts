// Matches SpotPriceResponse
export interface SpotPrice {
  id: number | null;
  created_at: string | null;
  chain: string;
  pair_token_a: string;
  pair_token_b: string;
  block_height: number;
  min_price: number;
  max_price: number;
  min_pool_id: string;
  max_pool_id: string;
}

// Matches SwapResponse
export interface Swap {
  chain: string;
  height: number;
  token_in: string;
  token_out: string;
  pool_id: string;
  amount_in: string;
  amount_out: string;
  base_fee: number;
  gas_cost: string;
}

// Matches ExpectedProfitResponse
export interface ExpectedProfit {
  token_a: string;
  token_b: string;
  surplus_a: string;
  surplus_b: string;
  max_slippage_token_amount_a: string;
  max_slippage_token_amount_b: string;
  min_token_amount_a: string;
  min_token_amount_b: string;
  token_usdc_price_a: number;
  token_usdc_price_b: number;
  min_usdc_amount_a: string;
  min_usdc_amount_b: string;
  min_total_amount_usdc: string;
  gas_cost_eth_slow: string;
  gas_cost_eth_fast: string;
  total_gas_cost_eth: string;
  eth_usdc_price: number;
  gas_cost_usdc_slow: string;
  gas_cost_usdc_fast: string;
  total_gas_cost_usdc: string;
}

// Matches CrossChainSingleHopResponse
export interface Signal {
  id: number;
  slow: Swap;
  fast: Swap;
  slow_prices_a_b: SpotPrice;
  slow_prices_a_usdc: SpotPrice | null;
  slow_prices_b_usdc: SpotPrice | null;
  slow_prices_eth_usdc: SpotPrice | null;
  expected_profit: ExpectedProfit;
  max_slippage_bps: number;
  congestion_risk_discount_bps: number;
  fast_prices_a_b_created_at: string | null;
}

// Matches RealizedProfitDetail — present only on trades recorded after the field was added
export interface RealizedProfitDetail {
  surplus_a: string;
  surplus_b: string;
  usdc_amount_a: string;
  usdc_amount_b: string;
  gas_amount_eth: string;
  gas_amount_usdc: string;
  token_usdc_price_a: number;
  token_usdc_price_b: number;
  gas_price_usdc: number;
}

// Matches SuccessfulTradeResponse
export interface SuccessfulTrade {
  id: number;
  signal: Signal;
  slow_tx_hash: string;
  fast_tx_hash: string;
  realized_profit_str: string;
  realized_profit_detail: RealizedProfitDetail | null;
}

// Matches FailedOnSlowTradeResponse
export interface FailedOnSlowTrade {
  id: number;
  signal: Signal;
  slow_tx_hash: string | null;
}

// Matches FailedOnFastTradeResponse
export interface FailedOnFastTrade {
  id: number;
  signal: Signal;
  slow_tx_hash: string;
  fast_tx_hash: string | null;
}

// Matches SignalTradeResultResponse tagged enum — slim version without embedded signal,
// used on the signal detail page where the signal is already loaded.
export type SignalTradeResult =
  | { trade_type: 'Successful'; id: number; slow_tx_hash: string; fast_tx_hash: string; realized_profit_str: string }
  | { trade_type: 'FailedOnSlow'; id: number; slow_tx_hash: string | null }
  | { trade_type: 'FailedOnFast'; id: number; slow_tx_hash: string; fast_tx_hash: string | null }

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

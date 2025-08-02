-- Create tables for pair prices and arbitrage signals

CREATE TABLE IF NOT EXISTS spot_prices (
    id BIGSERIAL PRIMARY KEY,
    token_a_symbol VARCHAR(50) NOT NULL,
    token_b_symbol VARCHAR(50) NOT NULL,
    block_height BIGINT NOT NULL,
    min_price TEXT NOT NULL,
    max_price TEXT NOT NULL,
    pool_id VARCHAR(100) NOT NULL,
    chain VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_spot_prices_pool_block ON spot_prices(pool_id, block_height DESC);
CREATE INDEX IF NOT EXISTS idx_spot_prices_block_height ON spot_prices(block_height DESC);
CREATE INDEX IF NOT EXISTS idx_spot_prices_chain ON spot_prices(chain);
CREATE INDEX IF NOT EXISTS idx_spot_prices_chain_block ON spot_prices(chain, block_height DESC);

CREATE DOMAIN uint_bps AS INTEGER
  CHECK (VALUE BETWEEN 0 AND 10000);

CREATE TABLE IF NOT EXISTS signals (
    id BIGSERIAL PRIMARY KEY,

    slow_chain VARCHAR(50) NOT NULL,
    slow_height BIGINT NOT NULL,
    slow_pool_id VARCHAR(100) NOT NULL,
    fast_chain VARCHAR(50) NOT NULL,
    fast_height BIGINT NOT NULL,
    fast_pool_id VARCHAR(100) NOT NULL,
    slow_swap_token_in_symbol VARCHAR(50) NOT NULL,
    slow_swap_token_out_symbol VARCHAR(50) NOT NULL,
    slow_swap_amount_in TEXT NOT NULL,
    slow_swap_amount_out TEXT NOT NULL,
    slow_swap_gas_cost TEXT NOT NULL,
    fast_swap_token_in_symbol VARCHAR(50) NOT NULL,
    fast_swap_token_out_symbol VARCHAR(50) NOT NULL,
    fast_swap_amount_in TEXT NOT NULL,
    fast_swap_amount_out TEXT NOT NULL,
    fast_swap_gas_cost TEXT NOT NULL,
    surplus_a TEXT NOT NULL,
    surplus_b TEXT NOT NULL,
    expected_profit_a TEXT NOT NULL,
    expected_profit_b TEXT NOT NULL,
    max_slippage_bps uint_bps NOT NULL,
    congestion_risk_discount_bps uint_bps NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_signals_slow_height ON signals(slow_height DESC);
CREATE INDEX IF NOT EXISTS idx_signals_fast_height ON signals(fast_height DESC);

CREATE INDEX IF NOT EXISTS idx_signals_slow_chain ON signals(slow_chain);
CREATE INDEX IF NOT EXISTS idx_signals_fast_chain ON signals(fast_chain);

CREATE INDEX IF NOT EXISTS idx_signals_slow_token_in_symbol ON signals(slow_swap_token_in_symbol);
CREATE INDEX IF NOT EXISTS idx_signals_slow_token_out_symbol ON signals(slow_swap_token_out_symbol);
CREATE INDEX IF NOT EXISTS idx_signals_fast_token_in_symbol ON signals(fast_swap_token_in_symbol);
CREATE INDEX IF NOT EXISTS idx_signals_fast_token_out_symbol ON signals(fast_swap_token_out_symbol);

CREATE INDEX IF NOT EXISTS idx_signals_created_at ON signals(created_at DESC);

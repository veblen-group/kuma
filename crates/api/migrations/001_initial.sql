-- Create tables for pair prices and arbitrage signals

CREATE TABLE IF NOT EXISTS pair_prices (
    id BIGSERIAL PRIMARY KEY,
    token_a_symbol VARCHAR(50) NOT NULL,
    token_a_address VARCHAR(100) NOT NULL,
    token_a_decimals INTEGER NOT NULL,
    token_b_symbol VARCHAR(50) NOT NULL,
    token_b_address VARCHAR(100) NOT NULL,
    token_b_decimals INTEGER NOT NULL,
    block_height BIGINT NOT NULL,
    price TEXT NOT NULL,
    pool_id VARCHAR(100) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_pair_prices_pool_block ON pair_prices(pool_id, block_height DESC);
CREATE INDEX IF NOT EXISTS idx_pair_prices_block_height ON pair_prices(block_height DESC);

CREATE TABLE IF NOT EXISTS arbitrage_signals (
    id BIGSERIAL PRIMARY KEY,
    block_height BIGINT NOT NULL,
    slow_chain VARCHAR(50) NOT NULL,
    slow_pair_token_a_symbol VARCHAR(50) NOT NULL,
    slow_pair_token_a_address VARCHAR(100) NOT NULL,
    slow_pair_token_a_decimals INTEGER NOT NULL,
    slow_pair_token_b_symbol VARCHAR(50) NOT NULL,
    slow_pair_token_b_address VARCHAR(100) NOT NULL,
    slow_pair_token_b_decimals INTEGER NOT NULL,
    slow_pool_id VARCHAR(100) NOT NULL,
    fast_chain VARCHAR(50) NOT NULL,
    fast_pair_token_a_symbol VARCHAR(50) NOT NULL,
    fast_pair_token_a_address VARCHAR(100) NOT NULL,
    fast_pair_token_a_decimals INTEGER NOT NULL,
    fast_pair_token_b_symbol VARCHAR(50) NOT NULL,
    fast_pair_token_b_address VARCHAR(100) NOT NULL,
    fast_pair_token_b_decimals INTEGER NOT NULL,
    fast_pool_id VARCHAR(100) NOT NULL,
    slow_swap_token_in_symbol VARCHAR(50) NOT NULL,
    slow_swap_token_in_address VARCHAR(100) NOT NULL,
    slow_swap_token_in_decimals INTEGER NOT NULL,
    slow_swap_token_out_symbol VARCHAR(50) NOT NULL,
    slow_swap_token_out_address VARCHAR(100) NOT NULL,
    slow_swap_token_out_decimals INTEGER NOT NULL,
    slow_swap_amount_in TEXT NOT NULL,
    slow_swap_amount_out TEXT NOT NULL,
    fast_swap_token_in_symbol VARCHAR(50) NOT NULL,
    fast_swap_token_in_address VARCHAR(100) NOT NULL,
    fast_swap_token_in_decimals INTEGER NOT NULL,
    fast_swap_token_out_symbol VARCHAR(50) NOT NULL,
    fast_swap_token_out_address VARCHAR(100) NOT NULL,
    fast_swap_token_out_decimals INTEGER NOT NULL,
    fast_swap_amount_in TEXT NOT NULL,
    fast_swap_amount_out TEXT NOT NULL,
    surplus_a TEXT NOT NULL,
    surplus_b TEXT NOT NULL,
    expected_profit_a TEXT NOT NULL,
    expected_profit_b TEXT NOT NULL,
    max_slippage_bps BIGINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_arbitrage_signals_block_height ON arbitrage_signals(block_height DESC);
CREATE INDEX IF NOT EXISTS idx_arbitrage_signals_created_at ON arbitrage_signals(created_at DESC);
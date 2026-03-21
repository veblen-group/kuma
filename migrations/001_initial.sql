-- Create tables for pair prices and arbitrage signals

CREATE TABLE IF NOT EXISTS spot_prices (
    id BIGSERIAL PRIMARY KEY,
    token_a_symbol VARCHAR(50) NOT NULL,
    token_b_symbol VARCHAR(50) NOT NULL,
    block_height BIGINT NOT NULL,
    min_price DOUBLE PRECISION NOT NULL,
    max_price DOUBLE PRECISION NOT NULL,
    min_pool_id VARCHAR(100) NOT NULL,
    max_pool_id VARCHAR(100) NOT NULL,
    chain VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_spot_prices_min_pool_height ON spot_prices(min_pool_id, block_height DESC);
CREATE INDEX IF NOT EXISTS idx_spot_prices_max_pool_height ON spot_prices(max_pool_id, block_height DESC);
CREATE INDEX IF NOT EXISTS idx_spot_prices_block_height ON spot_prices(block_height DESC);
CREATE INDEX IF NOT EXISTS idx_spot_prices_chain ON spot_prices(chain);
CREATE INDEX IF NOT EXISTS idx_spot_prices_chain_block ON spot_prices(chain, block_height DESC);
CREATE INDEX IF NOT EXISTS idx_spot_prices_chain_pair_height ON spot_prices(chain, token_a_symbol, token_b_symbol, block_height DESC);

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
    min_token_amount_a TEXT NOT NULL,
    min_token_amount_b TEXT NOT NULL,
    min_usdc_amount_a TEXT NOT NULL,
    min_usdc_amount_b TEXT NOT NULL,
    min_total_amount_usdc TEXT NOT NULL,
    max_slippage_token_amount_a TEXT NOT NULL,
    max_slippage_token_amount_b TEXT NOT NULL,
    token_usdc_price_a DOUBLE PRECISION NOT NULL,
    token_usdc_price_b DOUBLE PRECISION NOT NULL,
    gas_cost_eth_slow TEXT NOT NULL,
    gas_cost_eth_fast TEXT NOT NULL,
    total_gas_cost_eth TEXT NOT NULL,
    eth_usdc_price DOUBLE PRECISION NOT NULL,
    gas_cost_usdc_slow TEXT NOT NULL,
    gas_cost_usdc_fast TEXT NOT NULL,
    total_gas_cost_usdc TEXT NOT NULL,
    slow_base_fee BIGINT NOT NULL,
    fast_base_fee BIGINT NOT NULL,
    slow_prices_a_b_id BIGINT REFERENCES spot_prices(id),
    slow_prices_a_usdc_id BIGINT REFERENCES spot_prices(id),
    slow_prices_b_usdc_id BIGINT REFERENCES spot_prices(id),
    slow_prices_eth_usdc_id BIGINT REFERENCES spot_prices(id),
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


CREATE TABLE IF NOT EXISTS failed_on_slow_chain_trade (
    id BIGSERIAL PRIMARY KEY,
    signal_id BIGINT NOT NULL REFERENCES signals(id), -- Changed to NOT NULL
    slow_tx_hash VARCHAR(66), -- 0x + 64 hex chars, nullable
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_failed_on_slow_chain_trade_signal_id ON failed_on_slow_chain_trade(signal_id);
CREATE INDEX IF NOT EXISTS idx_failed_on_slow_chain_trade_slow_tx_hash ON failed_on_slow_chain_trade(slow_tx_hash);
CREATE INDEX IF NOT EXISTS idx_failed_on_slow_chain_trade_created_at ON failed_on_slow_chain_trade(created_at DESC);

CREATE TABLE IF NOT EXISTS failed_on_fast_chain_trade (
    id BIGSERIAL PRIMARY KEY,
    signal_id BIGINT NOT NULL REFERENCES signals(id),
    slow_tx_hash VARCHAR(66) NOT NULL, -- 0x + 64 hex chars
    fast_tx_hash VARCHAR(66), -- 0x + 64 hex chars, nullable
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_failed_on_fast_chain_trade_signal_id ON failed_on_fast_chain_trade(signal_id);
CREATE INDEX IF NOT EXISTS idx_failed_on_fast_chain_trade_slow_tx_hash ON failed_on_fast_chain_trade(slow_tx_hash);
CREATE INDEX IF NOT EXISTS idx_failed_on_fast_chain_trade_fast_tx_hash ON failed_on_fast_chain_trade(fast_tx_hash);
CREATE INDEX IF NOT EXISTS idx_failed_on_fast_chain_trade_created_at ON failed_on_fast_chain_trade(created_at DESC);

CREATE TABLE IF NOT EXISTS successful_trade (
    id BIGSERIAL PRIMARY KEY,
    signal_id BIGINT NOT NULL REFERENCES signals(id),
    slow_tx_hash VARCHAR(66) NOT NULL, -- 0x + 64 hex chars
    fast_tx_hash VARCHAR(66) NOT NULL, -- 0x + 64 hex chars
    realized_profit_str TEXT NOT NULL, -- BigUint as string
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_successful_trade_signal_id ON successful_trade(signal_id);
CREATE INDEX IF NOT EXISTS idx_successful_trade_slow_tx_hash ON successful_trade(slow_tx_hash);
CREATE INDEX IF NOT EXISTS idx_successful_trade_fast_tx_hash ON successful_trade(fast_tx_hash);
CREATE INDEX IF NOT EXISTS idx_successful_trade_created_at ON successful_trade(created_at DESC);

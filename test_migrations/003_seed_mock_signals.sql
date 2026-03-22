-- Insert mock signals for testing purposes
-- Uses INSERT ... SELECT so subqueries can resolve spot price FKs

-- Helper: a single-row VALUES table to allow subqueries alongside literal values
-- Pattern: INSERT INTO signals (...) SELECT ..., (SELECT id FROM spot_prices WHERE ...) ...

-- ── UNI/WETH signals (ethereum → unichain) ────────────────────────────────────

INSERT INTO signals (
    slow_chain, slow_height, slow_pool_id,
    fast_chain, fast_height, fast_pool_id,
    slow_swap_token_in_symbol, slow_swap_token_out_symbol,
    slow_swap_amount_in, slow_swap_amount_out, slow_swap_gas_cost,
    fast_swap_token_in_symbol, fast_swap_token_out_symbol,
    fast_swap_amount_in, fast_swap_amount_out, fast_swap_gas_cost,
    surplus_a, surplus_b,
    min_token_amount_a, min_token_amount_b,
    min_usdc_amount_a, min_usdc_amount_b, min_total_amount_usdc,
    max_slippage_token_amount_a, max_slippage_token_amount_b,
    token_usdc_price_a, token_usdc_price_b,
    gas_cost_eth_slow, gas_cost_eth_fast, total_gas_cost_eth,
    eth_usdc_price,
    gas_cost_usdc_slow, gas_cost_usdc_fast, total_gas_cost_usdc,
    slow_base_fee, fast_base_fee,
    max_slippage_bps, congestion_risk_discount_bps,
    slow_prices_a_b_id, slow_prices_eth_usdc_id
)
SELECT
    'ethereum', height_eth, '0x1111000000000000000000000000000000000001',
    'unichain', height_uni, '0x2222000000000000000000000000000000000001',
    token_in_slow, token_out_slow,
    amount_in_slow, amount_out_slow, '10000000000000',
    token_in_fast, token_out_fast,
    amount_in_fast, amount_out_fast, '8000000000000',
    surplus_a_val, surplus_b_val,
    min_a, min_b,
    '50', '130', '50',
    '8', '1000',
    9.3, 3700.0,
    '1000000000000', '800000000000', '1800000000000',
    3700.0,
    '3700000000000', '2960000000000', '6660000000000',
    10, 8,
    25, 0,
    (SELECT id FROM spot_prices WHERE token_a_symbol = 'UNI' AND token_b_symbol = 'WETH' AND chain = 'ethereum' ORDER BY block_height DESC LIMIT 1),
    (SELECT id FROM spot_prices WHERE token_a_symbol = 'USDC' AND token_b_symbol = 'WETH' AND chain = 'ethereum' ORDER BY block_height DESC LIMIT 1)
FROM (VALUES
    (19500000, 1234567, 'UNI', 'WETH', 'WETH', 'UNI', '1000000000000000000000', '2600000000000000', '2600000000000000', '1020000000000000000000', '20000000000000000000', '52000000000000', '18000000000000000000', '47000000000000'),
    (19500001, 1234568, 'WETH', 'UNI', 'UNI', 'WETH', '2600000000000000', '1000000000000000000000', '1000000000000000000000', '2650000000000000', '25000000000000000000', '65000000000000', '22000000000000000000', '57000000000000'),
    (19500002, 1234569, 'UNI', 'WETH', 'WETH', 'UNI', '2000000000000000000000', '5100000000000000', '5000000000000000', '2040000000000000000000', '40000000000000000000', '100000000000000', '35000000000000000000', '90000000000000'),
    (19500003, 1234570, 'WETH', 'UNI', 'UNI', 'WETH', '2700000000000000', '1050000000000000000000', '1050000000000000000000', '2755000000000000', '28000000000000000000', '71000000000000', '25000000000000000000', '64000000000000'),
    (19500004, 1234571, 'UNI', 'WETH', 'WETH', 'UNI', '1500000000000000000000', '3900000000000000', '3850000000000000', '1530000000000000000000', '30000000000000000000', '77000000000000', '27000000000000000000', '70000000000000'),
    (19500005, 1234572, 'WETH', 'UNI', 'UNI', 'WETH', '3200000000000000', '1240000000000000000000', '1240000000000000000000', '3265000000000000', '32000000000000000000', '81000000000000', '29000000000000000000', '74000000000000'),
    (19500006, 1234573, 'UNI', 'WETH', 'WETH', 'UNI', '800000000000000000000', '2080000000000000', '2060000000000000', '816000000000000000000', '16000000000000000000', '41000000000000', '14000000000000000000', '37000000000000'),
    (19500007, 1234574, 'WETH', 'UNI', 'UNI', 'WETH', '2100000000000000', '815000000000000000000', '815000000000000000000', '2140000000000000', '21000000000000000000', '54000000000000', '19000000000000000000', '49000000000000'),
    (19500008, 1234575, 'UNI', 'WETH', 'WETH', 'UNI', '3000000000000000000000', '7800000000000000', '7750000000000000', '3060000000000000000000', '60000000000000000000', '154000000000000', '54000000000000000000', '139000000000000'),
    (19500009, 1234576, 'WETH', 'UNI', 'UNI', 'WETH', '1800000000000000', '699000000000000000000', '699000000000000000000', '1835000000000000', '18000000000000000000', '46000000000000', '16000000000000000000', '42000000000000'),
    (19500010, 1234577, 'UNI', 'WETH', 'WETH', 'UNI', '1200000000000000000000', '3120000000000000', '3100000000000000', '1224000000000000000000', '24000000000000000000', '62000000000000', '22000000000000000000', '56000000000000'),
    (19500011, 1234578, 'WETH', 'UNI', 'UNI', 'WETH', '2450000000000000', '952000000000000000000', '952000000000000000000', '2497000000000000', '27000000000000000000', '69000000000000', '24000000000000000000', '63000000000000')
) AS v(height_eth, height_uni, token_in_slow, token_out_slow, token_in_fast, token_out_fast, amount_in_slow, amount_out_slow, amount_in_fast, amount_out_fast, surplus_a_val, surplus_b_val, min_a, min_b);

-- ── USDC/WETH signal (ethereum → base) ────────────────────────────────────────

INSERT INTO signals (
    slow_chain, slow_height, slow_pool_id,
    fast_chain, fast_height, fast_pool_id,
    slow_swap_token_in_symbol, slow_swap_token_out_symbol,
    slow_swap_amount_in, slow_swap_amount_out, slow_swap_gas_cost,
    fast_swap_token_in_symbol, fast_swap_token_out_symbol,
    fast_swap_amount_in, fast_swap_amount_out, fast_swap_gas_cost,
    surplus_a, surplus_b,
    min_token_amount_a, min_token_amount_b,
    min_usdc_amount_a, min_usdc_amount_b, min_total_amount_usdc,
    max_slippage_token_amount_a, max_slippage_token_amount_b,
    token_usdc_price_a, token_usdc_price_b,
    gas_cost_eth_slow, gas_cost_eth_fast, total_gas_cost_eth,
    eth_usdc_price,
    gas_cost_usdc_slow, gas_cost_usdc_fast, total_gas_cost_usdc,
    slow_base_fee, fast_base_fee,
    max_slippage_bps, congestion_risk_discount_bps,
    slow_prices_a_b_id, slow_prices_eth_usdc_id
)
SELECT
    'ethereum', 19500000, '0x3333000000000000000000000000000000000001',
    'base', 9876543, '0x6666000000000000000000000000000000000001',
    'USDC', 'WETH',
    '10000000000', '2700000000000000', '10000000000000',
    'WETH', 'USDC',
    '2700000000000000', '10200000000', '8000000000000',
    '200000000', '54000000000000',
    '180000000', '49000000000000',
    '50', '130', '50',
    '8', '1000',
    1.0, 3700.0,
    '1000000000000', '800000000000', '1800000000000',
    3700.0,
    '3700000000000', '2960000000000', '6660000000000',
    10, 8,
    25, 0,
    (SELECT id FROM spot_prices WHERE token_a_symbol = 'USDC' AND token_b_symbol = 'WETH' AND chain = 'ethereum' ORDER BY block_height DESC LIMIT 1),
    (SELECT id FROM spot_prices WHERE token_a_symbol = 'USDC' AND token_b_symbol = 'WETH' AND chain = 'ethereum' ORDER BY block_height DESC LIMIT 1);

-- ── UNI/USDC signal (ethereum → unichain) ─────────────────────────────────────

INSERT INTO signals (
    slow_chain, slow_height, slow_pool_id,
    fast_chain, fast_height, fast_pool_id,
    slow_swap_token_in_symbol, slow_swap_token_out_symbol,
    slow_swap_amount_in, slow_swap_amount_out, slow_swap_gas_cost,
    fast_swap_token_in_symbol, fast_swap_token_out_symbol,
    fast_swap_amount_in, fast_swap_amount_out, fast_swap_gas_cost,
    surplus_a, surplus_b,
    min_token_amount_a, min_token_amount_b,
    min_usdc_amount_a, min_usdc_amount_b, min_total_amount_usdc,
    max_slippage_token_amount_a, max_slippage_token_amount_b,
    token_usdc_price_a, token_usdc_price_b,
    gas_cost_eth_slow, gas_cost_eth_fast, total_gas_cost_eth,
    eth_usdc_price,
    gas_cost_usdc_slow, gas_cost_usdc_fast, total_gas_cost_usdc,
    slow_base_fee, fast_base_fee,
    max_slippage_bps, congestion_risk_discount_bps,
    slow_prices_a_b_id
)
SELECT
    'ethereum', 19500000, '0x4444000000000000000000000000000000000001',
    'unichain', 1234567, '0x7777000000000000000000000000000000000001',
    'UNI', 'USDC',
    '1000000000000000000000', '9300000000', '10000000000000',
    'USDC', 'UNI',
    '9300000000', '1010000000000000000000', '8000000000000',
    '10000000000000000000', '9300000',
    '9000000000000000000', '8400000',
    '50', '30', '50',
    '8', '1000',
    9.3, 1.0,
    '1000000000000', '800000000000', '1800000000000',
    3700.0,
    '3700000000000', '2960000000000', '6660000000000',
    10, 8,
    25, 0,
    (SELECT id FROM spot_prices WHERE token_a_symbol = 'UNI' AND token_b_symbol = 'USDC' AND chain = 'ethereum' ORDER BY block_height DESC LIMIT 1);

-- ── PEPE/WETH signal (base → ethereum) ────────────────────────────────────────

INSERT INTO signals (
    slow_chain, slow_height, slow_pool_id,
    fast_chain, fast_height, fast_pool_id,
    slow_swap_token_in_symbol, slow_swap_token_out_symbol,
    slow_swap_amount_in, slow_swap_amount_out, slow_swap_gas_cost,
    fast_swap_token_in_symbol, fast_swap_token_out_symbol,
    fast_swap_amount_in, fast_swap_amount_out, fast_swap_gas_cost,
    surplus_a, surplus_b,
    min_token_amount_a, min_token_amount_b,
    min_usdc_amount_a, min_usdc_amount_b, min_total_amount_usdc,
    max_slippage_token_amount_a, max_slippage_token_amount_b,
    token_usdc_price_a, token_usdc_price_b,
    gas_cost_eth_slow, gas_cost_eth_fast, total_gas_cost_eth,
    eth_usdc_price,
    gas_cost_usdc_slow, gas_cost_usdc_fast, total_gas_cost_usdc,
    slow_base_fee, fast_base_fee,
    max_slippage_bps, congestion_risk_discount_bps,
    slow_prices_a_b_id
)
SELECT
    'base', 19500000, '0x5555000000000000000000000000000000000001',
    'ethereum', 19500000, '0x8888000000000000000000000000000000000001',
    'PEPE', 'WETH',
    '1000000000000000000000000', '1100000000000000', '10000000000000',
    'WETH', 'PEPE',
    '1100000000000000', '1020000000000000000000000', '8000000000000',
    '20000000000000000000000', '22000000000000',
    '18000000000000000000000', '20000000000000',
    '50', '130', '50',
    '8', '1000',
    0.0000000011, 3700.0,
    '1000000000000', '800000000000', '1800000000000',
    3700.0,
    '3700000000000', '2960000000000', '6660000000000',
    10, 8,
    25, 0,
    (SELECT id FROM spot_prices WHERE token_a_symbol = 'PEPE' AND token_b_symbol = 'WETH' AND chain = 'base' ORDER BY block_height DESC LIMIT 1);

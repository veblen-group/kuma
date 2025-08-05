-- Insert mock data for spot prices for testing purposes

INSERT INTO spot_prices (
    token_a_symbol, token_b_symbol,
    block_height, min_price, max_price, min_pool_id, max_pool_id, chain
) VALUES
    -- WETH/USDC pair on Ethereum
    ('WETH', 'USDC',
     19500000, 3700, 3703, '0x123', '0x456', 'ethereum'),
    ('WETH', 'USDC',
     19500001, 3705, 3708, '0x123', '0xabc', 'ethereum'),
    ('WETH', 'USDC',
     19500002, 3695, 3698, '0x123', '0x789', 'ethereum'),

    -- PEPE/WETH pair on Base
    ('PEPE', 'WETH',
     19500000, 0.001, 0.0013, '0x456', '0xdef', 'base'),
    ('PEPE', 'WETH',
     19500001, 0.0012, 0.0015, '0x456', '0x123', 'base'),
    ('PEPE', 'WETH',
     19500002, 0.0009, 0.0012, '0x456', '0x234', 'base');

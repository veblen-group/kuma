-- Insert mock signals for testing purposes

INSERT INTO signals (
    slow_chain, slow_height, slow_pool_id,
    fast_chain, fast_height, fast_pool_id,
    slow_swap_token_in_symbol, slow_swap_token_out_symbol,
    slow_swap_amount_in, slow_swap_amount_out,
    fast_swap_token_in_symbol, fast_swap_token_out_symbol,
    fast_swap_amount_in, fast_swap_amount_out,
    surplus_a, surplus_b, expected_profit_a, expected_profit_b,
    max_slippage_bps, congestion_risk_discount_bps
) VALUES
    (
        'ethereum', 18765432, '0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984',
        'base', 9876543, '0x2f9840a85d5aF5bf1D1762F925BDADdC4201F985',
        'USDC', 'WETH',
        '10000', '5.2',
        'WETH', 'USDC',
        '5', '10200',
        '200', '200', '180', '180',
        50, 20
    ),
    (
        'base', 7654321, '0x3f9840a85d5aF5bf1D1762F925BDADdC4201F986',
        'ethereum', 3456789, '0x4f9840a85d5aF5bf1D1762F925BDADdC4201F987',
        'WETH', 'USDC',
        '10', '9800',
        'USDC', 'WETH',
        '9800', '10050',
        '250', '250', '220', '220',
        75, 30
    ),
    (
        'ethereum', 45678901, '0x5f9840a85d5aF5bf1D1762F925BDADdC4201F988',
        'base', 2345678, '0x6f9840a85d5aF5bf1D1762F925BDADdC4201F989',
        'USDC', 'WETH',
        '5000', '4.8',
        'WETH', 'USDC',
        '4.7', '5100',
        '400', '400', '350', '350',
        100, 40
    );

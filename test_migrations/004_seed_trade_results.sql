-- Insert mock trade result for testing
INSERT INTO successful_trade (
    signal_id, slow_tx_hash, fast_tx_hash, realized_profit_str
) VALUES (
    1, -- Mock signal_id
    '0x1234567890abcdef1234567890abcdef12345678',
    '0xabcdef1234567890abcdef1234567890abcdef12',
    '1000000000000000'
);

-- Insert another mock failed on fast chain trade result
INSERT INTO failed_on_fast_chain_trade (
    signal_id, slow_tx_hash, fast_tx_hash
) VALUES (
    1, -- Mock signal_id
    '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
    NULL -- fast_tx_hash is nullable in failed_on_fast_chain_trade
);

-- Insert another mock failed on slow chain trade result
INSERT INTO failed_on_slow_chain_trade (
    signal_id, slow_tx_hash
) VALUES (
    1, -- Mock signal_id
    NULL -- slow_tx_hash is nullable in failed_on_slow_chain_trade
);
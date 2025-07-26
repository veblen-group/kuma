use crate::models::{ArbitrageSignal, PairPrice};
use color_eyre::eyre::Result;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{info, instrument};

#[derive(Clone)]
pub struct PairPriceRepository {
    pool: Arc<PgPool>,
}

impl PairPriceRepository {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    #[instrument(skip(self, pair_price))]
    pub async fn insert(&self, pair_price: &PairPrice) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO pair_prices (
                token_a_symbol, token_a_address, token_a_decimals,
                token_b_symbol, token_b_address, token_b_decimals,
                block_height, price, pool_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#
        )
        .bind(&pair_price.pair.token_a.symbol)
        .bind(&pair_price.pair.token_a.address)
        .bind(pair_price.pair.token_a.decimals as i32)
        .bind(&pair_price.pair.token_b.symbol)
        .bind(&pair_price.pair.token_b.address)
        .bind(pair_price.pair.token_b.decimals as i32)
        .bind(pair_price.block_height as i64)
        .bind(&pair_price.price)
        .bind(&pair_price.pool_id)
        .execute(&*self.pool)
        .await?;

        info!("Inserted pair price for block {}", pair_price.block_height);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_latest_by_pool(&self, pool_id: &str) -> Result<Option<PairPrice>> {
        let row = sqlx::query(
            r#"
            SELECT 
                token_a_symbol, token_a_address, token_a_decimals,
                token_b_symbol, token_b_address, token_b_decimals,
                block_height, price, pool_id
            FROM pair_prices 
            WHERE pool_id = $1 
            ORDER BY block_height DESC 
            LIMIT 1
            "#
        )
        .bind(pool_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| PairPrice {
            pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: r.get("token_a_symbol"),
                    address: r.get("token_a_address"),
                    decimals: r.get::<i32, _>("token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: r.get("token_b_symbol"),
                    address: r.get("token_b_address"),
                    decimals: r.get::<i32, _>("token_b_decimals") as u32,
                },
            },
            block_height: r.get::<i64, _>("block_height") as u64,
            price: r.get("price"),
            pool_id: r.get("pool_id"),
        }))
    }

    #[instrument(skip(self))]
    pub async fn get_by_block_range(&self, pool_id: &str, start_block: u64, end_block: u64) -> Result<Vec<PairPrice>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                token_a_symbol, token_a_address, token_a_decimals,
                token_b_symbol, token_b_address, token_b_decimals,
                block_height, price, pool_id
            FROM pair_prices 
            WHERE pool_id = $1 AND block_height BETWEEN $2 AND $3
            ORDER BY block_height ASC
            "#
        )
        .bind(pool_id)
        .bind(start_block as i64)
        .bind(end_block as i64)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| PairPrice {
            pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: r.get("token_a_symbol"),
                    address: r.get("token_a_address"),
                    decimals: r.get::<i32, _>("token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: r.get("token_b_symbol"),
                    address: r.get("token_b_address"),
                    decimals: r.get::<i32, _>("token_b_decimals") as u32,
                },
            },
            block_height: r.get::<i64, _>("block_height") as u64,
            price: r.get("price"),
            pool_id: r.get("pool_id"),
        }).collect())
    }

    #[instrument(skip(self))]
    pub async fn get_by_block_height(&self, block_height: u64, pair_filter: Option<&str>) -> Result<Vec<PairPrice>> {
        let rows = match pair_filter {
            Some(pair) => {
                let parts: Vec<&str> = pair.split('-').collect();
                if parts.len() != 2 {
                    return Ok(vec![]);
                }
                let token_a = parts[0];
                let token_b = parts[1];
                
                sqlx::query(
                    r#"
                    SELECT 
                        token_a_symbol, token_a_address, token_a_decimals,
                        token_b_symbol, token_b_address, token_b_decimals,
                        block_height, price, pool_id
                    FROM pair_prices 
                    WHERE block_height = $1 
                    AND ((token_a_symbol = $2 AND token_b_symbol = $3) 
                         OR (token_a_symbol = $3 AND token_b_symbol = $2))
                    ORDER BY pool_id
                    "#
                )
                .bind(block_height as i64)
                .bind(token_a)
                .bind(token_b)
                .fetch_all(&*self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    r#"
                    SELECT 
                        token_a_symbol, token_a_address, token_a_decimals,
                        token_b_symbol, token_b_address, token_b_decimals,
                        block_height, price, pool_id
                    FROM pair_prices 
                    WHERE block_height = $1
                    ORDER BY pool_id
                    "#
                )
                .bind(block_height as i64)
                .fetch_all(&*self.pool)
                .await?
            }
        };

        Ok(rows.into_iter().map(|r| PairPrice {
            pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: r.get("token_a_symbol"),
                    address: r.get("token_a_address"),
                    decimals: r.get::<i32, _>("token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: r.get("token_b_symbol"),
                    address: r.get("token_b_address"),
                    decimals: r.get::<i32, _>("token_b_decimals") as u32,
                },
            },
            block_height: r.get::<i64, _>("block_height") as u64,
            price: r.get("price"),
            pool_id: r.get("pool_id"),
        }).collect())
    }
}

#[derive(Clone)]
pub struct ArbitrageSignalRepository {
    pool: Arc<PgPool>,
}

impl ArbitrageSignalRepository {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    #[instrument(skip(self, signal))]
    pub async fn insert(&self, signal: &ArbitrageSignal) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO arbitrage_signals (
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address, slow_pair_token_a_decimals,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pair_token_b_decimals, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address, fast_pair_token_a_decimals,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pair_token_b_decimals, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address, slow_swap_token_in_decimals,
                slow_swap_token_out_symbol, slow_swap_token_out_address, slow_swap_token_out_decimals,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address, fast_swap_token_in_decimals,
                fast_swap_token_out_symbol, fast_swap_token_out_address, fast_swap_token_out_decimals,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33,
                $34, $35, $36, $37, $38
            )
            "#
        )
        .bind(signal.block_height as i64)
        .bind(&signal.slow_chain)
        .bind(&signal.slow_pair.token_a.symbol)
        .bind(&signal.slow_pair.token_a.address)
        .bind(signal.slow_pair.token_a.decimals as i32)
        .bind(&signal.slow_pair.token_b.symbol)
        .bind(&signal.slow_pair.token_b.address)
        .bind(signal.slow_pair.token_b.decimals as i32)
        .bind(&signal.slow_pool_id)
        .bind(&signal.fast_chain)
        .bind(&signal.fast_pair.token_a.symbol)
        .bind(&signal.fast_pair.token_a.address)
        .bind(signal.fast_pair.token_a.decimals as i32)
        .bind(&signal.fast_pair.token_b.symbol)
        .bind(&signal.fast_pair.token_b.address)
        .bind(signal.fast_pair.token_b.decimals as i32)
        .bind(&signal.fast_pool_id)
        .bind(&signal.slow_swap.token_in.symbol)
        .bind(&signal.slow_swap.token_in.address)
        .bind(signal.slow_swap.token_in.decimals as i32)
        .bind(&signal.slow_swap.token_out.symbol)
        .bind(&signal.slow_swap.token_out.address)
        .bind(signal.slow_swap.token_out.decimals as i32)
        .bind(&signal.slow_swap.amount_in)
        .bind(&signal.slow_swap.amount_out)
        .bind(&signal.fast_swap.token_in.symbol)
        .bind(&signal.fast_swap.token_in.address)
        .bind(signal.fast_swap.token_in.decimals as i32)
        .bind(&signal.fast_swap.token_out.symbol)
        .bind(&signal.fast_swap.token_out.address)
        .bind(signal.fast_swap.token_out.decimals as i32)
        .bind(&signal.fast_swap.amount_in)
        .bind(&signal.fast_swap.amount_out)
        .bind(&signal.surplus_a)
        .bind(&signal.surplus_b)
        .bind(&signal.expected_profit_a)
        .bind(&signal.expected_profit_b)
        .bind(signal.max_slippage_bps as i64)
        .execute(&*self.pool)
        .await?;

        info!("Inserted arbitrage signal for block {}", signal.block_height);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_recent(&self, limit: u32) -> Result<Vec<ArbitrageSignal>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address, slow_pair_token_a_decimals,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pair_token_b_decimals, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address, fast_pair_token_a_decimals,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pair_token_b_decimals, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address, slow_swap_token_in_decimals,
                slow_swap_token_out_symbol, slow_swap_token_out_address, slow_swap_token_out_decimals,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address, fast_swap_token_in_decimals,
                fast_swap_token_out_symbol, fast_swap_token_out_address, fast_swap_token_out_decimals,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals 
            ORDER BY block_height DESC 
            LIMIT $1
            "#
        )
        .bind(limit as i64)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| ArbitrageSignal {
            block_height: row.get::<i64, _>("block_height") as u64,
            slow_chain: row.get("slow_chain"),
            slow_pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: row.get("slow_pair_token_a_symbol"),
                    address: row.get("slow_pair_token_a_address"),
                    decimals: row.get::<i32, _>("slow_pair_token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: row.get("slow_pair_token_b_symbol"),
                    address: row.get("slow_pair_token_b_address"),
                    decimals: row.get::<i32, _>("slow_pair_token_b_decimals") as u32,
                },
            },
            slow_pool_id: row.get("slow_pool_id"),
            fast_chain: row.get("fast_chain"),
            fast_pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: row.get("fast_pair_token_a_symbol"),
                    address: row.get("fast_pair_token_a_address"),
                    decimals: row.get::<i32, _>("fast_pair_token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: row.get("fast_pair_token_b_symbol"),
                    address: row.get("fast_pair_token_b_address"),
                    decimals: row.get::<i32, _>("fast_pair_token_b_decimals") as u32,
                },
            },
            fast_pool_id: row.get("fast_pool_id"),
            slow_swap: crate::models::SwapInfo {
                token_in: crate::models::Token {
                    symbol: row.get("slow_swap_token_in_symbol"),
                    address: row.get("slow_swap_token_in_address"),
                    decimals: row.get::<i32, _>("slow_swap_token_in_decimals") as u32,
                },
                token_out: crate::models::Token {
                    symbol: row.get("slow_swap_token_out_symbol"),
                    address: row.get("slow_swap_token_out_address"),
                    decimals: row.get::<i32, _>("slow_swap_token_out_decimals") as u32,
                },
                amount_in: row.get("slow_swap_amount_in"),
                amount_out: row.get("slow_swap_amount_out"),
            },
            fast_swap: crate::models::SwapInfo {
                token_in: crate::models::Token {
                    symbol: row.get("fast_swap_token_in_symbol"),
                    address: row.get("fast_swap_token_in_address"),
                    decimals: row.get::<i32, _>("fast_swap_token_in_decimals") as u32,
                },
                token_out: crate::models::Token {
                    symbol: row.get("fast_swap_token_out_symbol"),
                    address: row.get("fast_swap_token_out_address"),
                    decimals: row.get::<i32, _>("fast_swap_token_out_decimals") as u32,
                },
                amount_in: row.get("fast_swap_amount_in"),
                amount_out: row.get("fast_swap_amount_out"),
            },
            surplus_a: row.get("surplus_a"),
            surplus_b: row.get("surplus_b"),
            expected_profit_a: row.get("expected_profit_a"),
            expected_profit_b: row.get("expected_profit_b"),
            max_slippage_bps: row.get::<i64, _>("max_slippage_bps") as u64,
        }).collect())
    }

    #[instrument(skip(self))]
    pub async fn get_by_block_height(&self, block_height: u64, limit: u32, offset: u32) -> Result<Vec<ArbitrageSignal>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address, slow_pair_token_a_decimals,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pair_token_b_decimals, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address, fast_pair_token_a_decimals,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pair_token_b_decimals, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address, slow_swap_token_in_decimals,
                slow_swap_token_out_symbol, slow_swap_token_out_address, slow_swap_token_out_decimals,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address, fast_swap_token_in_decimals,
                fast_swap_token_out_symbol, fast_swap_token_out_address, fast_swap_token_out_decimals,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals 
            WHERE block_height = $1
            ORDER BY id
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(block_height as i64)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| ArbitrageSignal {
            block_height: row.get::<i64, _>("block_height") as u64,
            slow_chain: row.get("slow_chain"),
            slow_pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: row.get("slow_pair_token_a_symbol"),
                    address: row.get("slow_pair_token_a_address"),
                    decimals: row.get::<i32, _>("slow_pair_token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: row.get("slow_pair_token_b_symbol"),
                    address: row.get("slow_pair_token_b_address"),
                    decimals: row.get::<i32, _>("slow_pair_token_b_decimals") as u32,
                },
            },
            slow_pool_id: row.get("slow_pool_id"),
            fast_chain: row.get("fast_chain"),
            fast_pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: row.get("fast_pair_token_a_symbol"),
                    address: row.get("fast_pair_token_a_address"),
                    decimals: row.get::<i32, _>("fast_pair_token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: row.get("fast_pair_token_b_symbol"),
                    address: row.get("fast_pair_token_b_address"),
                    decimals: row.get::<i32, _>("fast_pair_token_b_decimals") as u32,
                },
            },
            fast_pool_id: row.get("fast_pool_id"),
            slow_swap: crate::models::SwapInfo {
                token_in: crate::models::Token {
                    symbol: row.get("slow_swap_token_in_symbol"),
                    address: row.get("slow_swap_token_in_address"),
                    decimals: row.get::<i32, _>("slow_swap_token_in_decimals") as u32,
                },
                token_out: crate::models::Token {
                    symbol: row.get("slow_swap_token_out_symbol"),
                    address: row.get("slow_swap_token_out_address"),
                    decimals: row.get::<i32, _>("slow_swap_token_out_decimals") as u32,
                },
                amount_in: row.get("slow_swap_amount_in"),
                amount_out: row.get("slow_swap_amount_out"),
            },
            fast_swap: crate::models::SwapInfo {
                token_in: crate::models::Token {
                    symbol: row.get("fast_swap_token_in_symbol"),
                    address: row.get("fast_swap_token_in_address"),
                    decimals: row.get::<i32, _>("fast_swap_token_in_decimals") as u32,
                },
                token_out: crate::models::Token {
                    symbol: row.get("fast_swap_token_out_symbol"),
                    address: row.get("fast_swap_token_out_address"),
                    decimals: row.get::<i32, _>("fast_swap_token_out_decimals") as u32,
                },
                amount_in: row.get("fast_swap_amount_in"),
                amount_out: row.get("fast_swap_amount_out"),
            },
            surplus_a: row.get("surplus_a"),
            surplus_b: row.get("surplus_b"),
            expected_profit_a: row.get("expected_profit_a"),
            expected_profit_b: row.get("expected_profit_b"),
            max_slippage_bps: row.get::<i64, _>("max_slippage_bps") as u64,
        }).collect())
    }

    #[instrument(skip(self))]
    pub async fn get_by_block_range(&self, start_block: u64, end_block: u64) -> Result<Vec<ArbitrageSignal>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                block_height, slow_chain, slow_pair_token_a_symbol, slow_pair_token_a_address, slow_pair_token_a_decimals,
                slow_pair_token_b_symbol, slow_pair_token_b_address, slow_pair_token_b_decimals, slow_pool_id,
                fast_chain, fast_pair_token_a_symbol, fast_pair_token_a_address, fast_pair_token_a_decimals,
                fast_pair_token_b_symbol, fast_pair_token_b_address, fast_pair_token_b_decimals, fast_pool_id,
                slow_swap_token_in_symbol, slow_swap_token_in_address, slow_swap_token_in_decimals,
                slow_swap_token_out_symbol, slow_swap_token_out_address, slow_swap_token_out_decimals,
                slow_swap_amount_in, slow_swap_amount_out,
                fast_swap_token_in_symbol, fast_swap_token_in_address, fast_swap_token_in_decimals,
                fast_swap_token_out_symbol, fast_swap_token_out_address, fast_swap_token_out_decimals,
                fast_swap_amount_in, fast_swap_amount_out,
                surplus_a, surplus_b, expected_profit_a, expected_profit_b, max_slippage_bps
            FROM arbitrage_signals 
            WHERE block_height BETWEEN $1 AND $2
            ORDER BY block_height ASC
            "#
        )
        .bind(start_block as i64)
        .bind(end_block as i64)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| ArbitrageSignal {
            block_height: row.get::<i64, _>("block_height") as u64,
            slow_chain: row.get("slow_chain"),
            slow_pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: row.get("slow_pair_token_a_symbol"),
                    address: row.get("slow_pair_token_a_address"),
                    decimals: row.get::<i32, _>("slow_pair_token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: row.get("slow_pair_token_b_symbol"),
                    address: row.get("slow_pair_token_b_address"),
                    decimals: row.get::<i32, _>("slow_pair_token_b_decimals") as u32,
                },
            },
            slow_pool_id: row.get("slow_pool_id"),
            fast_chain: row.get("fast_chain"),
            fast_pair: crate::models::Pair {
                token_a: crate::models::Token {
                    symbol: row.get("fast_pair_token_a_symbol"),
                    address: row.get("fast_pair_token_a_address"),
                    decimals: row.get::<i32, _>("fast_pair_token_a_decimals") as u32,
                },
                token_b: crate::models::Token {
                    symbol: row.get("fast_pair_token_b_symbol"),
                    address: row.get("fast_pair_token_b_address"),
                    decimals: row.get::<i32, _>("fast_pair_token_b_decimals") as u32,
                },
            },
            fast_pool_id: row.get("fast_pool_id"),
            slow_swap: crate::models::SwapInfo {
                token_in: crate::models::Token {
                    symbol: row.get("slow_swap_token_in_symbol"),
                    address: row.get("slow_swap_token_in_address"),
                    decimals: row.get::<i32, _>("slow_swap_token_in_decimals") as u32,
                },
                token_out: crate::models::Token {
                    symbol: row.get("slow_swap_token_out_symbol"),
                    address: row.get("slow_swap_token_out_address"),
                    decimals: row.get::<i32, _>("slow_swap_token_out_decimals") as u32,
                },
                amount_in: row.get("slow_swap_amount_in"),
                amount_out: row.get("slow_swap_amount_out"),
            },
            fast_swap: crate::models::SwapInfo {
                token_in: crate::models::Token {
                    symbol: row.get("fast_swap_token_in_symbol"),
                    address: row.get("fast_swap_token_in_address"),
                    decimals: row.get::<i32, _>("fast_swap_token_in_decimals") as u32,
                },
                token_out: crate::models::Token {
                    symbol: row.get("fast_swap_token_out_symbol"),
                    address: row.get("fast_swap_token_out_address"),
                    decimals: row.get::<i32, _>("fast_swap_token_out_decimals") as u32,
                },
                amount_in: row.get("fast_swap_amount_in"),
                amount_out: row.get("fast_swap_amount_out"),
            },
            surplus_a: row.get("surplus_a"),
            surplus_b: row.get("surplus_b"),
            expected_profit_a: row.get("expected_profit_a"),
            expected_profit_b: row.get("expected_profit_b"),
            max_slippage_bps: row.get::<i64, _>("max_slippage_bps") as u64,
        }).collect())
    }
}
use serde::{Deserialize, Serialize};

pub const DEFAULT_PAGE_SIZE: u32 = 20;
pub const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub page_size: u32,
    pub total_pages: Option<u32>,
    pub total_items: Option<u64>,
    pub has_next: bool,
    pub has_previous: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaginationQuery {
    #[serde(deserialize_with = "deserialize_optional_u32", default)]
    pub page: Option<u32>,
    #[serde(deserialize_with = "deserialize_optional_u32", default)]
    pub page_size: Option<u32>,
}

fn deserialize_optional_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    match Option::<String>::deserialize(deserializer)? {
        Some(s) => s.parse().map(Some).map_err(D::Error::custom),
        None => Ok(None),
    }
}

impl PaginationQuery {
    pub fn sanitize(&self) -> (u32, u32) {
        let page = self.page.unwrap_or(1).max(1);
        let page_size = self
            .page_size
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .min(MAX_PAGE_SIZE)
            .max(1);
        (page, page_size)
    }

    pub fn to_offset_limit(&self) -> (u32, u32) {
        let (page, page_size) = self.sanitize();
        let offset = (page - 1) * page_size;
        (offset, page_size)
    }
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, page: u32, page_size: u32, total_items: Option<u64>) -> Self {
        let total_pages = total_items.map(|total| {
            if total == 0 {
                1
            } else {
                ((total - 1) / page_size as u64 + 1) as u32
            }
        });

        let has_next = total_items
            .map(|total| (page as u64 * page_size as u64) < total)
            .unwrap_or(!data.is_empty() && data.len() == page_size as usize);

        let has_previous = page > 1;

        Self {
            data,
            pagination: PaginationInfo {
                page,
                page_size,
                total_pages,
                total_items,
                has_next,
                has_previous,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pair {
    pub token_a: Token,
    pub token_b: Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Token {
    pub symbol: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotPrice {
    pub pair: Pair,
    pub block_height: u64,
    pub price: String, // BigUint as string
    pub pool_id: String,
    pub chain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageSignal {
    pub block_height: u64,
    pub slow_chain: String,
    pub slow_pair: Pair,
    pub slow_pool_id: String,
    pub fast_chain: String,
    pub fast_pair: Pair,
    pub fast_pool_id: String,
    pub slow_swap: SwapInfo,
    pub fast_swap: SwapInfo,
    pub surplus_a: String,         // BigUint as string
    pub surplus_b: String,         // BigUint as string
    pub expected_profit_a: String, // BigUint as string
    pub expected_profit_b: String, // BigUint as string
    pub max_slippage_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapInfo {
    pub token_in: Token,
    pub token_out: Token,
    pub amount_in: String,  // BigUint as string
    pub amount_out: String, // BigUint as string
}

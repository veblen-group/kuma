# API Reference

This document provides comprehensive reference documentation for Kuma's REST API endpoints.

## Base URL

**Development:** `http://localhost:8080`  
**Production:** Configure according to your deployment

## Authentication

Currently, the API does not require authentication for development. For production deployments, implement appropriate authentication mechanisms.

## Response Format

All API responses follow a consistent JSON format:

```json
{
  "data": [...],           // Response data (array for collections, object for single items)
  "pagination": {          // Pagination info (only for paginated endpoints)
    "page": 1,
    "page_size": 10,
    "total": 1000,
    "total_pages": 100
  },
  "meta": {               // Additional metadata (optional)
    "request_id": "uuid",
    "timestamp": "2024-01-01T12:00:00Z"
  }
}
```

## Error Responses

Error responses use standard HTTP status codes with detailed error information:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid parameter: page must be a positive integer",
    "details": {
      "parameter": "page",
      "value": "-1",
      "expected": "positive integer"
    }
  }
}
```

**Common HTTP Status Codes:**
- `200 OK` - Request successful
- `400 Bad Request` - Invalid parameters
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server error

## Endpoints

### Spot Prices

#### GET /spot_prices

Retrieve historical spot price data for token pairs.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| pair | string | No | Token pair filter (e.g., "WETH-USDC") |
| chain | string | No | Blockchain filter ("ethereum", "unichain") |
| page | integer | No | Page number (default: 1) |
| page_size | integer | No | Items per page (default: 10, max: 100) |
| from_block | integer | No | Minimum block height |
| to_block | integer | No | Maximum block height |

**Example Request:**
```bash
curl "http://localhost:8080/spot_prices?pair=WETH-USDC&chain=ethereum&page=1&page_size=20"
```

**Example Response:**
```json
{
  "data": [
    {
      "id": 12345,
      "token_a_symbol": "WETH",
      "token_b_symbol": "USDC",
      "block_height": 18500000,
      "min_price": 1800.50,
      "max_price": 1802.75,
      "min_pool_id": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
      "max_pool_id": "0xa374094527e1673a86de625aa59517c5de346d32",
      "chain": "ethereum",
      "created_at": "2024-01-01T12:00:00Z"
    },
    {
      "id": 12346,
      "token_a_symbol": "WETH", 
      "token_b_symbol": "USDC",
      "block_height": 18500001,
      "min_price": 1801.25,
      "max_price": 1803.10,
      "min_pool_id": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
      "max_pool_id": "0xa374094527e1673a86de625aa59517c5de346d32",
      "chain": "ethereum",
      "created_at": "2024-01-01T12:01:00Z"
    }
  ],
  "pagination": {
    "page": 1,
    "page_size": 20,
    "total": 15420,
    "total_pages": 771
  }
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| id | integer | Unique spot price record ID |
| token_a_symbol | string | First token symbol |
| token_b_symbol | string | Second token symbol |
| block_height | integer | Blockchain block height |
| min_price | number | Minimum observed price |
| max_price | number | Maximum observed price |
| min_pool_id | string | Pool ID with minimum price |
| max_pool_id | string | Pool ID with maximum price |
| chain | string | Blockchain network name |
| created_at | string | ISO 8601 timestamp |

#### GET /spot_prices/{id}

Retrieve a specific spot price record by ID.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| id | integer | Yes | Spot price record ID |

**Example Request:**
```bash
curl "http://localhost:8080/spot_prices/12345"
```

**Example Response:**
```json
{
  "data": {
    "id": 12345,
    "token_a_symbol": "WETH",
    "token_b_symbol": "USDC",
    "block_height": 18500000,
    "min_price": 1800.50,
    "max_price": 1802.75,
    "min_pool_id": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
    "max_pool_id": "0xa374094527e1673a86de625aa59517c5de346d32",
    "chain": "ethereum",
    "created_at": "2024-01-01T12:00:00Z"
  }
}
```

### Signals

#### GET /signals

Retrieve arbitrage signal data.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| slow_chain | string | No | Slow chain filter |
| fast_chain | string | No | Fast chain filter |
| token_a | string | No | First token symbol filter |
| token_b | string | No | Second token symbol filter |
| min_profit | number | No | Minimum expected profit filter |
| page | integer | No | Page number (default: 1) |
| page_size | integer | No | Items per page (default: 10, max: 100) |
| from_date | string | No | ISO 8601 start date |
| to_date | string | No | ISO 8601 end date |

**Example Request:**
```bash
curl "http://localhost:8080/signals?slow_chain=ethereum&fast_chain=unichain&min_profit=100&page=1&page_size=10"
```

**Example Response:**
```json
{
  "data": [
    {
      "id": 567,
      "slow_chain": "ethereum",
      "slow_height": 18500000,
      "slow_pool_id": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
      "fast_chain": "unichain",
      "fast_height": 2100000,
      "fast_pool_id": "0xa374094527e1673a86de625aa59517c5de346d32",
      "slow_swap_token_in_symbol": "WETH",
      "slow_swap_token_out_symbol": "UNI",
      "slow_swap_amount_in": "1000000000000000000",
      "slow_swap_amount_out": "555000000000000000000",
      "slow_swap_gas_cost": "50000000000000000",
      "fast_swap_token_in_symbol": "UNI",
      "fast_swap_token_out_symbol": "WETH",
      "fast_swap_amount_in": "555000000000000000000",
      "fast_swap_amount_out": "1020000000000000000",
      "fast_swap_gas_cost": "25000000000000000",
      "surplus_a": "20000000000000000",
      "surplus_b": "0",
      "expected_profit_a": "125500000000000000",
      "expected_profit_b": "0",
      "max_slippage_bps": 25,
      "congestion_risk_discount_bps": 0,
      "created_at": "2024-01-01T12:00:00Z"
    }
  ],
  "pagination": {
    "page": 1,
    "page_size": 10,
    "total": 245,
    "total_pages": 25
  }
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| id | integer | Unique signal ID |
| slow_chain | string | Source chain for arbitrage |
| slow_height | integer | Block height on slow chain |
| slow_pool_id | string | Pool ID on slow chain |
| fast_chain | string | Target chain for arbitrage |
| fast_height | integer | Block height on fast chain |
| fast_pool_id | string | Pool ID on fast chain |
| slow_swap_* | string | Swap details on slow chain |
| fast_swap_* | string | Swap details on fast chain |
| surplus_a | string | Token A surplus amount |
| surplus_b | string | Token B surplus amount |
| expected_profit_a | string | Expected profit in token A |
| expected_profit_b | string | Expected profit in token B |
| max_slippage_bps | integer | Maximum slippage in basis points |
| congestion_risk_discount_bps | integer | Congestion risk discount |
| created_at | string | ISO 8601 timestamp |

#### GET /signals/{id}

Retrieve a specific signal by ID.

**Example Request:**
```bash
curl "http://localhost:8080/signals/567"
```

**Example Response:**
```json
{
  "data": {
    "id": 567,
    "slow_chain": "ethereum",
    // ... full signal data
  }
}
```

### Health and Status

#### GET /health

System health check endpoint.

**Example Request:**
```bash
curl "http://localhost:8080/health"
```

**Example Response:**
```json
{
  "status": "healthy",
  "timestamp": "2024-01-01T12:00:00Z",
  "version": "1.0.0",
  "uptime": 86400,
  "checks": {
    "database": {
      "status": "healthy",
      "response_time_ms": 5,
      "connections_active": 3,
      "connections_max": 10
    },
    "blockchain": {
      "ethereum": {
        "status": "healthy",
        "block_height": 18500000,
        "response_time_ms": 150
      },
      "unichain": {
        "status": "healthy", 
        "block_height": 2100000,
        "response_time_ms": 75
      }
    }
  }
}
```

#### GET /metrics

Prometheus-compatible metrics endpoint (if enabled).

**Example Request:**
```bash
curl "http://localhost:8080/metrics"
```

**Example Response:**
```text
# HELP kuma_signals_total Total number of signals generated
# TYPE kuma_signals_total counter
kuma_signals_total 1234

# HELP kuma_spot_prices_total Total number of spot prices collected
# TYPE kuma_spot_prices_total counter
kuma_spot_prices_total 56789

# HELP kuma_database_connections_active Active database connections
# TYPE kuma_database_connections_active gauge
kuma_database_connections_active 3
```

## SDK Examples

### JavaScript/TypeScript

```typescript
// Install: npm install axios

import axios from 'axios';

interface SpotPrice {
  id: number;
  token_a_symbol: string;
  token_b_symbol: string;
  min_price: number;
  max_price: number;
  chain: string;
  created_at: string;
}

interface Signal {
  id: number;
  slow_chain: string;
  fast_chain: string;
  expected_profit_a: string;
  expected_profit_b: string;
  created_at: string;
}

class KumaAPI {
  private baseURL: string;

  constructor(baseURL: string = 'http://localhost:8080') {
    this.baseURL = baseURL;
  }

  async getSpotPrices(params: {
    pair?: string;
    chain?: string;
    page?: number;
    page_size?: number;
  }): Promise<SpotPrice[]> {
    const response = await axios.get(`${this.baseURL}/spot_prices`, { params });
    return response.data.data;
  }

  async getSignals(params: {
    slow_chain?: string;
    fast_chain?: string;
    min_profit?: number;
    page?: number;
    page_size?: number;
  }): Promise<Signal[]> {
    const response = await axios.get(`${this.baseURL}/signals`, { params });
    return response.data.data;
  }

  async getHealth(): Promise<any> {
    const response = await axios.get(`${this.baseURL}/health`);
    return response.data;
  }
}

// Usage
const api = new KumaAPI();

// Get recent WETH-USDC prices
const prices = await api.getSpotPrices({
  pair: 'WETH-USDC',
  chain: 'ethereum',
  page: 1,
  page_size: 50
});

// Get profitable signals
const signals = await api.getSignals({
  min_profit: 100,
  page: 1,
  page_size: 20
});
```

### Python

```python
# Install: pip install requests

import requests
from typing import List, Dict, Optional
from dataclasses import dataclass
from datetime import datetime

@dataclass
class SpotPrice:
    id: int
    token_a_symbol: str
    token_b_symbol: str
    min_price: float
    max_price: float
    chain: str
    created_at: datetime

@dataclass
class Signal:
    id: int
    slow_chain: str
    fast_chain: str
    expected_profit_a: str
    expected_profit_b: str
    created_at: datetime

class KumaAPI:
    def __init__(self, base_url: str = "http://localhost:8080"):
        self.base_url = base_url
        self.session = requests.Session()

    def get_spot_prices(self, 
                       pair: Optional[str] = None,
                       chain: Optional[str] = None,
                       page: int = 1,
                       page_size: int = 10) -> List[SpotPrice]:
        params = {
            'page': page,
            'page_size': page_size
        }
        if pair:
            params['pair'] = pair
        if chain:
            params['chain'] = chain

        response = self.session.get(f"{self.base_url}/spot_prices", params=params)
        response.raise_for_status()
        
        data = response.json()['data']
        return [SpotPrice(**item) for item in data]

    def get_signals(self,
                   slow_chain: Optional[str] = None,
                   fast_chain: Optional[str] = None,
                   min_profit: Optional[float] = None,
                   page: int = 1,
                   page_size: int = 10) -> List[Signal]:
        params = {
            'page': page,
            'page_size': page_size
        }
        if slow_chain:
            params['slow_chain'] = slow_chain
        if fast_chain:
            params['fast_chain'] = fast_chain
        if min_profit:
            params['min_profit'] = min_profit

        response = self.session.get(f"{self.base_url}/signals", params=params)
        response.raise_for_status()
        
        data = response.json()['data']
        return [Signal(**item) for item in data]

    def get_health(self) -> Dict:
        response = self.session.get(f"{self.base_url}/health")
        response.raise_for_status()
        return response.json()

# Usage
api = KumaAPI()

# Get recent prices
prices = api.get_spot_prices(pair='WETH-USDC', chain='ethereum', page_size=50)

# Get profitable signals
signals = api.get_signals(min_profit=100, page_size=20)

# Check system health
health = api.get_health()
print(f"System status: {health['status']}")
```

### Rust

```rust
// Cargo.toml: reqwest = { version = "0.11", features = ["json"] }

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct SpotPrice {
    pub id: i64,
    pub token_a_symbol: String,
    pub token_b_symbol: String,
    pub min_price: f64,
    pub max_price: f64,
    pub chain: String,
    pub created_at: String,
}

#[derive(Deserialize, Debug)]
pub struct Signal {
    pub id: i64,
    pub slow_chain: String,
    pub fast_chain: String,
    pub expected_profit_a: String,
    pub expected_profit_b: String,
    pub created_at: String,
}

#[derive(Deserialize, Debug)]
pub struct ApiResponse<T> {
    pub data: T,
    pub pagination: Option<Pagination>,
}

#[derive(Deserialize, Debug)]
pub struct Pagination {
    pub page: i32,
    pub page_size: i32,
    pub total: i32,
    pub total_pages: i32,
}

pub struct KumaAPI {
    client: Client,
    base_url: String,
}

impl KumaAPI {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.unwrap_or_else(|| "http://localhost:8080".to_string()),
        }
    }

    pub async fn get_spot_prices(&self, params: HashMap<String, String>) 
        -> Result<Vec<SpotPrice>, reqwest::Error> {
        let url = format!("{}/spot_prices", self.base_url);
        let response: ApiResponse<Vec<SpotPrice>> = self.client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        
        Ok(response.data)
    }

    pub async fn get_signals(&self, params: HashMap<String, String>) 
        -> Result<Vec<Signal>, reqwest::Error> {
        let url = format!("{}/signals", self.base_url);
        let response: ApiResponse<Vec<Signal>> = self.client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        
        Ok(response.data)
    }
}

// Usage
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = KumaAPI::new(None);
    
    let mut params = HashMap::new();
    params.insert("pair".to_string(), "WETH-USDC".to_string());
    params.insert("chain".to_string(), "ethereum".to_string());
    
    let prices = api.get_spot_prices(params).await?;
    println!("Retrieved {} spot prices", prices.len());
    
    Ok(())
}
```

## Rate Limits

Currently, there are no enforced rate limits in development. For production deployments, implement rate limiting based on your requirements:

**Recommended Limits:**
- General endpoints: 100 requests/minute
- Health checks: 300 requests/minute  
- Bulk data endpoints: 20 requests/minute

## Error Codes

| Code | Description |
|------|-------------|
| `VALIDATION_ERROR` | Invalid request parameters |
| `NOT_FOUND` | Requested resource not found |
| `DATABASE_ERROR` | Database operation failed |
| `INTERNAL_ERROR` | Unexpected server error |
| `RATE_LIMIT_EXCEEDED` | Too many requests |

This API reference provides comprehensive documentation for integrating with the Kuma system programmatically.
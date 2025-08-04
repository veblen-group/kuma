use color_eyre::eyre::{self, eyre};

/// Parses a string like "USDC-WETH" into ("USDC", "WETH").
///
/// # Errors
/// - `EmptyInput` if the trimmed input is empty.
/// - `InvalidFormat` if there isn’t exactly one `-`.
/// - `EmptyTokenA` or `EmptyTokenB` if either side is empty after trimming.
pub fn parse_pair(input: &str) -> eyre::Result<(String, String)> {
    // 1) Trim overall input
    let s = input.trim();
    if s.is_empty() {
        return Err(eyre!("empty input"));
    }

    // 2) Split on '-' and ensure exactly two parts
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        return Err(eyre!("invalid format"));
    }

    // 3) Trim each token
    let token_a = parts[0].trim();
    let token_b = parts[1].trim();

    // 4) Validate non-empty tokens
    if token_a.is_empty() {
        return Err(eyre!("empty token A"));
    }
    if token_b.is_empty() {
        return Err(eyre!("empty token B"));
    }

    Ok((token_a.to_string(), token_b.to_string()))
}

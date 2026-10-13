use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|v| v.with_timezone(&Utc))
}
pub fn canonical_address(chain: &str, address: &str) -> String {
    if chain == "bsc" {
        address.to_ascii_lowercase()
    } else {
        address.to_owned()
    }
}
pub fn valid_address(chain: &str, address: &str) -> bool {
    match chain {
        "bsc" => {
            address.len() == 42
                && address.starts_with("0x")
                && address[2..].bytes().all(|v| v.is_ascii_hexdigit())
        }
        "solana" => bs58::decode(address)
            .into_vec()
            .map(|v| v.len() == 32)
            .unwrap_or(false),
        _ => false,
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TokenSnapshot {
    pub chain: String,
    pub token_address: String,
    pub pool_address: String,
    pub name: String,
    pub symbol: String,
    pub dex: String,
    pub price_usd: Option<f64>,
    pub liquidity_usd: Option<f64>,
    pub volume_24h_usd: Option<f64>,
    pub market_cap_usd: Option<f64>,
    pub fdv_usd: Option<f64>,
    pub buys_5m: Option<u64>,
    pub sells_5m: Option<u64>,
    pub buys_1h: Option<u64>,
    pub sells_1h: Option<u64>,
    pub price_change_5m_pct: Option<f64>,
    pub price_change_1h_pct: Option<f64>,
    pub pool_created_at: Option<String>,
    pub fetched_at: String,
    pub source: String,
    pub source_url: String,
    pub discovery_labels: Vec<String>,
}
impl TokenSnapshot {
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.chain,
            canonical_address(&self.chain, &self.token_address),
            canonical_address(&self.chain, &self.pool_address)
        )
    }
    pub fn valid(&self) -> bool {
        valid_address(&self.chain, &self.token_address)
            && !self.pool_address.is_empty()
            && timestamp(&self.fetched_at).is_some()
            && [
                self.price_usd,
                self.liquidity_usd,
                self.volume_24h_usd,
                self.market_cap_usd,
                self.fdv_usd,
            ]
            .iter()
            .flatten()
            .all(|v| v.is_finite() && *v >= 0.0)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceStatus {
    pub id: String,
    pub status: String,
    pub checked_at: String,
    pub last_success_at: Option<String>,
    pub latency_ms: Option<u64>,
    pub requests: u64,
    pub errors: u64,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub at: String,
    pub price_usd: Option<f64>,
    pub liquidity_usd: Option<f64>,
    pub volume_24h_usd: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal {
    pub id: String,
    pub chain: String,
    pub token_address: String,
    pub pool_address: String,
    pub rule: String,
    pub severity: String,
    pub at: String,
    pub state: String,
    pub message: String,
    pub evidence: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenView {
    #[serde(flatten)]
    pub token: TokenSnapshot,
    pub label: String,
    pub state: String,
    pub first_seen_at: String,
    pub why: Vec<String>,
    pub evidence: Vec<String>,
    pub incomplete_data: Vec<String>,
    pub invalidation: Vec<String>,
    pub history: Vec<Point>,
    pub timeline: Vec<Signal>,
    pub outcome: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub mode: String,
    pub generated_at_utc: String,
    pub max_staleness_seconds: u64,
    pub sources: Vec<SourceStatus>,
    pub items: Vec<TokenView>,
    pub provider_errors: Vec<String>,
    pub signals: Vec<Signal>,
    pub metrics: serde_json::Value,
}
impl Snapshot {
    pub fn empty(mode: &str) -> Self {
        Self {
            mode: mode.into(),
            generated_at_utc: now(),
            max_staleness_seconds: 2700,
            sources: vec![],
            items: vec![],
            provider_errors: vec![],
            signals: vec![],
            metrics: serde_json::json!({}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn address_validation_rejects_wrong_chain_and_short_mint() {
        assert!(valid_address(
            "solana",
            "So11111111111111111111111111111111111111112"
        ));
        assert!(valid_address(
            "bsc",
            "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c"
        ));
        assert!(!valid_address("solana", "So1111"));
        assert!(!valid_address(
            "ethereum",
            "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c"
        ));
        assert!(!valid_address("bsc", "0x你好"));
    }
}

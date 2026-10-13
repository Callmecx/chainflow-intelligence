//! Public, read-only providers. Network health and timestamps are never fabricated.
mod http;
mod parsing;
pub use parsing::deduplicate;

use crate::models::{canonical_address, now, valid_address, SourceStatus, TokenSnapshot};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use http::{Bucket, Http};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub const SOL_BASELINE: &str = "So11111111111111111111111111111111111111112";
pub const BSC_BASELINE: &str = "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c";
const DEX: &str = "https://api.dexscreener.com";
const GECKO: &str = "https://api.geckoterminal.com/api/v2";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderResult {
    pub items: Vec<TokenSnapshot>,
    pub sources: Vec<SourceStatus>,
    pub provider_errors: Vec<String>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn scan(&self, watchlist: &[(String, String)]) -> ProviderResult;
    async fn lookup(&self, chain: &str, address: &str) -> ProviderResult;
}

pub struct ProviderHub {
    providers: Vec<Arc<dyn Provider>>,
    http: Arc<Http>,
}
impl ProviderHub {
    pub fn new() -> Result<Self> {
        let http = Http::new()?;
        Ok(Self {
            providers: vec![
                Arc::new(DexProvider { http: http.clone() }),
                Arc::new(GeckoProvider { http: http.clone() }),
                Arc::new(RpcProvider { http: http.clone() }),
            ],
            http,
        })
    }
    pub async fn scan(&self, watchlist: &[(String, String)]) -> ProviderResult {
        let (dex, gecko, rpc) = tokio::join!(
            self.providers[0].scan(watchlist),
            self.providers[1].scan(watchlist),
            self.providers[2].scan(watchlist)
        );
        self.combine(vec![dex, gecko, rpc]).await
    }
    pub async fn lookup(&self, chain: &str, address: &str) -> ProviderResult {
        if !valid_address(chain, address) {
            return ProviderResult {
                provider_errors: vec!["invalid chain/address".into()],
                ..Default::default()
            };
        }
        let (dex, gecko) = tokio::join!(
            self.providers[0].lookup(chain, address),
            self.providers[1].lookup(chain, address)
        );
        self.combine(vec![dex, gecko]).await
    }
    async fn combine(&self, results: Vec<ProviderResult>) -> ProviderResult {
        let mut result = ProviderResult::default();
        for value in results {
            result.items.extend(value.items);
            result.sources.extend(value.sources);
            result.provider_errors.extend(value.provider_errors);
        }
        result.items = deduplicate(result.items);
        result.sources.extend(self.http.statuses().await);
        result.sources.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }
}
#[async_trait]
impl Provider for ProviderHub {
    async fn scan(&self, watchlist: &[(String, String)]) -> ProviderResult {
        ProviderHub::scan(self, watchlist).await
    }
    async fn lookup(&self, chain: &str, address: &str) -> ProviderResult {
        ProviderHub::lookup(self, chain, address).await
    }
}

/// Explicit offline injection only; production never falls back to this provider.
pub struct FixtureProvider {
    replay: ProviderResult,
}
impl FixtureProvider {
    pub fn from_json(text: &str) -> Result<Self> {
        Ok(Self {
            replay: serde_json::from_str(text)?,
        })
    }
}
#[async_trait]
impl Provider for FixtureProvider {
    async fn scan(&self, _watchlist: &[(String, String)]) -> ProviderResult {
        self.replay.clone()
    }
    async fn lookup(&self, chain: &str, address: &str) -> ProviderResult {
        let mut result = self.replay.clone();
        result
            .items
            .retain(|v| v.chain == chain && v.token_address == canonical_address(chain, address));
        result
    }
}

struct DexProvider {
    http: Arc<Http>,
}
impl DexProvider {
    async fn fetch_pairs(
        &self,
        chain: &str,
        url: &str,
        result: &mut ProviderResult,
    ) -> Vec<TokenSnapshot> {
        let id = format!("dexscreener_market_{chain}");
        match self.http.get(&id, url, Bucket::Market, 20).await {
            Ok(response) => match parsing::dex(&response.body, chain, &response.fetched_at, url) {
                Ok(rows) => {
                    if rows.is_empty() {
                        self.http.no_coverage(&id,"HTTP success, no valid base-token pool coverage for requested chain".into()).await;
                    }
                    rows
                }
                Err(error) => {
                    self.http.schema_error(&id, error.to_string()).await;
                    result.provider_errors.push(format!("{id}: {error}"));
                    vec![]
                }
            },
            Err(error) => {
                result
                    .provider_errors
                    .push(format!("dexscreener_market/{chain}: {error}"));
                vec![]
            }
        }
    }
}
#[async_trait]
impl Provider for DexProvider {
    async fn scan(&self, watchlist: &[(String, String)]) -> ProviderResult {
        let mut result = ProviderResult::default();
        let mut addresses: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut priority: HashMap<(String, String), usize> = HashMap::new();
        addresses.insert(
            ("solana".into(), SOL_BASELINE.into()),
            vec!["BASELINE".into()],
        );
        addresses.insert(("bsc".into(), BSC_BASELINE.into()), vec!["BASELINE".into()]);
        for (index, (chain, address)) in watchlist.iter().enumerate() {
            if valid_address(chain, address) {
                let key = (chain.clone(), canonical_address(chain, address));
                priority.entry(key.clone()).or_insert(index);
                addresses.entry(key).or_default().push("TRACKED".into());
            }
        }
        for (path, id, label) in [
            (
                "token-profiles/latest/v1",
                "dexscreener_profiles",
                "PROFILE",
            ),
            ("token-boosts/latest/v1", "dexscreener_boosts", "BOOSTED"),
        ] {
            let url = format!("{DEX}/{path}");
            match self.http.get(id, &url, Bucket::Discovery, 45).await {
                Ok(response) => {
                    if let Some(rows) = response.body.as_array() {
                        let mut per_chain: HashMap<&str, usize> = HashMap::new();
                        for row in rows {
                            let chain = row["chainId"].as_str().unwrap_or_default();
                            let address = row["tokenAddress"].as_str().unwrap_or_default();
                            if valid_address(chain, address)
                                && *per_chain.get(chain).unwrap_or(&0) < 8
                            {
                                *per_chain.entry(chain).or_default() += 1;
                                addresses
                                    .entry((chain.into(), canonical_address(chain, address)))
                                    .or_default()
                                    .push(label.into());
                            }
                        }
                    } else {
                        self.http
                            .schema_error(id, "expected discovery array".into())
                            .await;
                        result
                            .provider_errors
                            .push(format!("{id}: invalid discovery schema"));
                    }
                }
                Err(error) => result.provider_errors.push(format!("{id}: {error}")),
            }
        }
        for chain in ["solana", "bsc"] {
            // Caller orders personal watchlist before recent discoveries. Preserve that order.
            let mut batch: Vec<_> = addresses.iter().filter(|((c, _), _)| c == chain).collect();
            batch.sort_by_key(|(key, labels)| {
                if labels.iter().any(|v| v == "BASELINE") {
                    0
                } else {
                    priority
                        .get(*key)
                        .copied()
                        .map(|v| v + 1)
                        .unwrap_or(usize::MAX)
                }
            });
            let selected: Vec<_> = batch.into_iter().take(90).collect();
            let omitted_watchlist = addresses
                .iter()
                .filter(|((c, a), labels)| {
                    c == chain
                        && labels.iter().any(|v| v == "TRACKED")
                        && !selected.iter().any(|((_, s), _)| s == a)
                })
                .count();
            if omitted_watchlist > 0 {
                result.provider_errors.push(format!("dexscreener/{chain}: {omitted_watchlist} tracked entries exceed 90-address per-scan budget"));
            }
            let keys: Vec<_> = selected
                .iter()
                .map(|((_, address), _)| address.as_str())
                .collect();
            let mut rows = vec![];
            for chunk in keys.chunks(30) {
                let url = format!("{DEX}/tokens/v1/{chain}/{}", chunk.join(","));
                rows.extend(self.fetch_pairs(chain, &url, &mut result).await);
            }
            rows.retain(|row| keys.contains(&row.token_address.as_str()));
            rows.sort_by(|a, b| {
                b.liquidity_usd
                    .partial_cmp(&a.liquidity_usd)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut per_token: HashMap<String, usize> = HashMap::new();
            for mut row in rows {
                let count = per_token.entry(row.token_address.clone()).or_default();
                if *count >= 4 {
                    continue;
                }
                *count += 1;
                row.discovery_labels = addresses
                    .get(&(chain.into(), row.token_address.clone()))
                    .cloned()
                    .unwrap_or_default();
                result.items.push(row);
            }
        }
        result
    }
    async fn lookup(&self, chain: &str, address: &str) -> ProviderResult {
        let mut result = ProviderResult::default();
        if !valid_address(chain, address) {
            return result;
        }
        let url = format!("{DEX}/token-pairs/v1/{chain}/{address}");
        result.items = self
            .fetch_pairs(chain, &url, &mut result)
            .await
            .into_iter()
            .filter(|v| v.token_address == canonical_address(chain, address))
            .take(20)
            .collect();
        result
    }
}

struct GeckoProvider {
    http: Arc<Http>,
}
impl GeckoProvider {
    async fn pools(
        &self,
        chain: &str,
        path: &str,
        id: &str,
        label: &str,
        result: &mut ProviderResult,
    ) {
        let url = format!("{GECKO}/{path}?include=base_token,quote_token,dex");
        match self.http.get(id, &url, Bucket::Gecko, 60).await {
            Ok(response) => {
                match parsing::gecko(&response.body, chain, &response.fetched_at, &url, label) {
                    Ok(rows) => {
                        if rows.is_empty() {
                            self.http.no_coverage(id,"HTTP success, no valid base-token pool coverage for requested chain".into()).await;
                        }
                        result.items.extend(rows)
                    }
                    Err(error) => {
                        self.http.schema_error(id, error.to_string()).await;
                        result.provider_errors.push(format!("{id}: {error}"));
                    }
                }
            }
            Err(error) => result.provider_errors.push(format!("{id}: {error}")),
        }
    }
}
#[async_trait]
impl Provider for GeckoProvider {
    async fn scan(&self, _watchlist: &[(String, String)]) -> ProviderResult {
        let mut result = ProviderResult::default();
        for chain in ["solana", "bsc"] {
            for (endpoint, label) in [("new_pools", "NEW_POOL"), ("trending_pools", "TRENDING")] {
                self.pools(
                    chain,
                    &format!("networks/{chain}/{endpoint}"),
                    &format!("geckoterminal_{chain}_{endpoint}"),
                    label,
                    &mut result,
                )
                .await;
            }
        }
        result
    }
    async fn lookup(&self, chain: &str, address: &str) -> ProviderResult {
        let mut result = ProviderResult::default();
        if !valid_address(chain, address) {
            return result;
        }
        self.pools(
            chain,
            &format!("networks/{chain}/tokens/{address}/pools"),
            &format!("geckoterminal_{chain}_lookup"),
            "EXACT_LOOKUP",
            &mut result,
        )
        .await;
        result
            .items
            .retain(|v| v.token_address == canonical_address(chain, address));
        result
    }
}

struct RpcProvider {
    http: Arc<Http>,
}
#[async_trait]
impl Provider for RpcProvider {
    async fn scan(&self, _watchlist: &[(String, String)]) -> ProviderResult {
        let mut result = ProviderResult::default();
        let sol=self.http.post("solana_rpc","https://api.mainnet.solana.com",json!({"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":[SOL_BASELINE,{"encoding":"jsonParsed","commitment":"confirmed"}]})).await;
        match sol {
            Ok(response) => {
                if let Some(slot) = response.body["result"]["context"]["slot"]
                    .as_u64()
                    .filter(|_| !response.body["result"]["value"].is_null())
                {
                    result.sources.push(SourceStatus{id:"solana_mint_verification".into(),status:"OK".into(),checked_at:response.fetched_at.clone(),last_success_at:Some(response.fetched_at),detail:format!("getAccountInfo mint={SOL_BASELINE}; slot={slot}; commitment=confirmed; no transaction signature because this is an account read"),..Default::default()});
                } else {
                    let detail = format!(
                        "invalid account response: {}",
                        response.body.get("error").unwrap_or(&Value::Null)
                    );
                    self.http.schema_error("solana_rpc", detail.clone()).await;
                    result.provider_errors.push(format!("solana_rpc: {detail}"));
                }
            }
            Err(error) => result.provider_errors.push(format!("solana_rpc: {error}")),
        }
        let bsc=self.http.post("bsc_rpc","https://bsc-dataseed.bnbchain.org",json!([
            {"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]},
            {"jsonrpc":"2.0","id":2,"method":"eth_blockNumber","params":[]},
            {"jsonrpc":"2.0","id":3,"method":"eth_call","params":[{"to":BSC_BASELINE,"data":"0x313ce567"},"latest"]}
        ])).await;
        match bsc {
            Ok(response) => {
                let by_id: HashMap<u64, &Value> = response
                    .body
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| Some((v["id"].as_u64()?, v)))
                    .collect();
                let chain = by_id.get(&1).and_then(|v| v["result"].as_str());
                let block = by_id.get(&2).and_then(|v| v["result"].as_str());
                let decimals = by_id.get(&3).and_then(|v| v["result"].as_str());
                if chain == Some("0x38")
                    && block.is_some()
                    && decimals.is_some_and(|v| v.ends_with("12"))
                {
                    result.sources.push(SourceStatus{id:"bsc_contract_verification".into(),status:"OK".into(),checked_at:response.fetched_at.clone(),last_success_at:Some(response.fetched_at),detail:format!("eth_chainId=0x38; eth_blockNumber={}; eth_call WBNB decimals=18; latest (not finality proof)",block.unwrap_or_default()),..Default::default()});
                } else {
                    let detail =
                        "BSC chain/eth_call response invalid or RPC method unavailable".to_string();
                    self.http.schema_error("bsc_rpc", detail.clone()).await;
                    result.provider_errors.push(format!("bsc_rpc: {detail}"));
                }
            }
            Err(error) => result.provider_errors.push(format!("bsc_rpc: {error}")),
        }
        result.sources.push(SourceStatus{id:"bsc_event_logs".into(),status:"UNSUPPORTED".into(),checked_at:now(),detail:"Official public BSC endpoints document eth_getLogs disabled; continuous full-chain swaps are unsupported".into(),..Default::default()});
        result
    }
    async fn lookup(&self, _chain: &str, _address: &str) -> ProviderResult {
        ProviderResult::default()
    }
}

/// Optional typed market history operations share Gecko's same bucket/cache.
/// Data is returned with source/time metadata; empty arrays remain empty.
impl ProviderHub {
    pub async fn dex_search_address(
        &self,
        chain: &str,
        address: &str,
    ) -> Result<Vec<TokenSnapshot>> {
        if !valid_address(chain, address) {
            return Err(anyhow!("invalid chain/token address"));
        }
        let url = format!("{DEX}/latest/dex/search?q={address}");
        let response = self
            .http
            .get("dexscreener_market", &url, Bucket::Market, 20)
            .await?;
        Ok(
            parsing::dex(&response.body, chain, &response.fetched_at, &url)?
                .into_iter()
                .filter(|v| v.token_address == canonical_address(chain, address))
                .collect(),
        )
    }
    pub async fn dex_pool(&self, chain: &str, pool: &str) -> Result<Vec<TokenSnapshot>> {
        if !valid_address(chain, pool) {
            return Err(anyhow!("invalid chain/pool address"));
        }
        let url = format!("{DEX}/latest/dex/pairs/{chain}/{pool}");
        let response = self
            .http
            .get("dexscreener_market", &url, Bucket::Market, 20)
            .await?;
        parsing::dex(&response.body, chain, &response.fetched_at, &url)
    }
    pub async fn pool_ohlcv(&self, chain: &str, pool: &str) -> Result<Value> {
        if !valid_address(chain, pool) {
            return Err(anyhow!("invalid chain/pool address"));
        }
        let url=format!("{GECKO}/networks/{chain}/pools/{pool}/ohlcv/minute?aggregate=5&limit=100&currency=usd&token=base");
        let response = self
            .http
            .get("geckoterminal_ohlcv", &url, Bucket::Gecko, 60)
            .await?;
        let list = response.body["data"]["attributes"]["ohlcv_list"]
            .as_array()
            .ok_or_else(|| anyhow!("invalid OHLCV response"))?;
        Ok(
            json!({"source":"geckoterminal","source_url":url,"fetched_at":response.fetched_at,"kind":"PROVIDER_OHLCV","candles":list}),
        )
    }
    pub async fn pool_trades(&self, chain: &str, pool: &str) -> Result<Value> {
        if !valid_address(chain, pool) {
            return Err(anyhow!("invalid chain/pool address"));
        }
        let url = format!("{GECKO}/networks/{chain}/pools/{pool}/trades");
        let response = self
            .http
            .get("geckoterminal_trades", &url, Bucket::Gecko, 60)
            .await?;
        let trades = response.body["data"]
            .as_array()
            .ok_or_else(|| anyhow!("invalid trades response"))?;
        Ok(
            json!({"source":"geckoterminal","source_url":url,"fetched_at":response.fetched_at,"kind":"RECENT_PROVIDER_TRADES","trades":trades}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn fixture_replay_is_deterministic_and_explicit() {
        let fixture = FixtureProvider::from_json(
            r#"{"items":[],"sources":[],"provider_errors":["OFFLINE_FIXTURE"]}"#,
        )
        .unwrap();
        let provider: &dyn Provider = &fixture;
        assert_eq!(
            provider.scan(&[]).await.provider_errors,
            vec!["OFFLINE_FIXTURE"]
        );
    }
    #[tokio::test]
    async fn invalid_lookup_never_makes_a_network_request() {
        let hub = ProviderHub::new().unwrap();
        let result = hub.lookup("ethereum", "bad").await;
        assert_eq!(result.provider_errors, vec!["invalid chain/address"]);
        assert!(result.sources.is_empty());
    }
}

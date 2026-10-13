use crate::models::{canonical_address, valid_address, TokenSnapshot};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| v.is_finite())
}
fn money(value: &Value) -> Option<f64> {
    number(value).filter(|v| *v >= 0.0)
}
fn count(value: &Value) -> Option<u64> {
    value.as_u64()
}

pub(super) fn dex(
    body: &Value,
    chain: &str,
    fetched_at: &str,
    url: &str,
) -> Result<Vec<TokenSnapshot>> {
    let rows = body
        .as_array()
        .or_else(|| body.get("pairs").and_then(Value::as_array))
        .ok_or_else(|| anyhow!("DEX schema: expected array/pairs"))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            if row["chainId"].as_str() != Some(chain) {
                return None;
            }
            let token_address = canonical_address(chain, &text(&row["baseToken"]["address"]));
            if !valid_address(chain, &token_address) {
                return None;
            }
            let item = TokenSnapshot {
                chain: chain.into(),
                token_address,
                pool_address: canonical_address(chain, &text(&row["pairAddress"])),
                name: text(&row["baseToken"]["name"]),
                symbol: text(&row["baseToken"]["symbol"]),
                dex: text(&row["dexId"]),
                price_usd: money(&row["priceUsd"]),
                liquidity_usd: money(&row["liquidity"]["usd"]),
                volume_24h_usd: money(&row["volume"]["h24"]),
                market_cap_usd: money(&row["marketCap"]),
                fdv_usd: money(&row["fdv"]),
                buys_5m: count(&row["txns"]["m5"]["buys"]),
                sells_5m: count(&row["txns"]["m5"]["sells"]),
                buys_1h: count(&row["txns"]["h1"]["buys"]),
                sells_1h: count(&row["txns"]["h1"]["sells"]),
                price_change_5m_pct: number(&row["priceChange"]["m5"]),
                price_change_1h_pct: number(&row["priceChange"]["h1"]),
                pool_created_at: row["pairCreatedAt"]
                    .as_i64()
                    .and_then(DateTime::<Utc>::from_timestamp_millis)
                    .map(|v| v.to_rfc3339()),
                fetched_at: fetched_at.into(),
                source: "dexscreener".into(),
                source_url: url.into(),
                discovery_labels: vec![],
            };
            item.valid().then_some(item)
        })
        .collect())
}
pub(super) fn gecko(
    body: &Value,
    chain: &str,
    fetched_at: &str,
    url: &str,
    label: &str,
) -> Result<Vec<TokenSnapshot>> {
    let rows = body["data"]
        .as_array()
        .ok_or_else(|| anyhow!("Gecko schema: expected data array"))?;
    let included: HashMap<String, &Value> = body["included"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| (text(&v["id"]), v))
        .collect();
    let prefix = format!("{chain}_");
    Ok(rows
        .iter()
        .take(12)
        .filter_map(|row| {
            if !text(&row["id"]).starts_with(&prefix) {
                return None;
            }
            let base_id = text(&row["relationships"]["base_token"]["data"]["id"]);
            let token_address = canonical_address(chain, base_id.strip_prefix(&prefix)?);
            if !valid_address(chain, &token_address) {
                return None;
            }
            let attr = &row["attributes"];
            let token = included.get(&base_id).map(|v| &v["attributes"]);
            let fallback = text(&attr["name"])
                .split(" / ")
                .next()
                .unwrap_or_default()
                .to_string();
            let item = TokenSnapshot {
                chain: chain.into(),
                token_address,
                pool_address: canonical_address(chain, &text(&attr["address"])),
                name: token
                    .map(|v| text(&v["name"]))
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| fallback.clone()),
                symbol: token
                    .map(|v| text(&v["symbol"]))
                    .filter(|v| !v.is_empty())
                    .unwrap_or(fallback),
                dex: text(&row["relationships"]["dex"]["data"]["id"]),
                price_usd: money(&attr["base_token_price_usd"]),
                liquidity_usd: money(&attr["reserve_in_usd"]),
                volume_24h_usd: money(&attr["volume_usd"]["h24"]),
                market_cap_usd: money(&attr["market_cap_usd"]),
                fdv_usd: money(&attr["fdv_usd"]),
                buys_5m: count(&attr["transactions"]["m5"]["buys"]),
                sells_5m: count(&attr["transactions"]["m5"]["sells"]),
                buys_1h: count(&attr["transactions"]["h1"]["buys"]),
                sells_1h: count(&attr["transactions"]["h1"]["sells"]),
                price_change_5m_pct: number(&attr["price_change_percentage"]["m5"]),
                price_change_1h_pct: number(&attr["price_change_percentage"]["h1"]),
                pool_created_at: attr["pool_created_at"]
                    .as_str()
                    .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
                    .map(|v| v.to_rfc3339()),
                fetched_at: fetched_at.into(),
                source: "geckoterminal".into(),
                source_url: url.into(),
                discovery_labels: vec![label.into()],
            };
            item.valid().then_some(item)
        })
        .collect())
}
/// Never aggregate pool FDV/market cap. Choose one coherent, latest observation per entity.
pub fn deduplicate(items: Vec<TokenSnapshot>) -> Vec<TokenSnapshot> {
    let mut unique: BTreeMap<String, TokenSnapshot> = BTreeMap::new();
    for mut item in items.into_iter().filter(TokenSnapshot::valid) {
        item.token_address = canonical_address(&item.chain, &item.token_address);
        item.pool_address = canonical_address(&item.chain, &item.pool_address);
        let key = item.key();
        if let Some(previous) = unique.remove(&key) {
            let mut labels = previous.discovery_labels.clone();
            labels.extend(item.discovery_labels.clone());
            labels.sort();
            labels.dedup();
            let previous_time = DateTime::parse_from_rfc3339(&previous.fetched_at).ok();
            let item_time = DateTime::parse_from_rfc3339(&item.fetched_at).ok();
            if previous_time > item_time {
                item = previous;
            }
            item.discovery_labels = labels;
        }
        unique.insert(key, item);
    }
    unique.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn row() -> Value {
        json!({"chainId":"bsc","pairAddress":"0x16b9a82891338f9bA80E2D6970FddA79D1eb0daE","baseToken":{"address":"0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c","name":"Wrapped BNB","symbol":"WBNB"},"priceUsd":"0.001","fdv":100,"liquidity":{"usd":0.00000001}})
    }
    #[test]
    fn absent_market_cap_stays_unknown_and_tiny_liquidity_stays_finite() {
        let rows = dex(&json!([row()]), "bsc", "2026-01-01T00:00:00Z", "fixture").unwrap();
        assert_eq!(rows[0].market_cap_usd, None);
        assert_eq!(rows[0].fdv_usd, Some(100.));
        assert_eq!(rows[0].liquidity_usd, Some(0.00000001));
    }
    #[test]
    fn reject_wrong_chain_and_nonfinite_values() {
        assert!(
            dex(&json!([row()]), "solana", "2026-01-01T00:00:00Z", "fixture")
                .unwrap()
                .is_empty()
        );
        let mut row = row();
        row["priceUsd"] = json!("NaN");
        assert_eq!(
            dex(&json!([row]), "bsc", "2026-01-01T00:00:00Z", "fixture").unwrap()[0].price_usd,
            None
        );
    }
    #[test]
    fn cross_source_case_normalization_and_out_of_order_samples() {
        let first = dex(&json!([row()]), "bsc", "2026-01-01T00:01:00Z", "fixture")
            .unwrap()
            .remove(0);
        let mut older = first.clone();
        older.source = "geckoterminal".into();
        older.fetched_at = "2026-01-01T00:00:00Z".into();
        older.price_usd = Some(5.);
        older.discovery_labels = vec!["NEW_POOL".into()];
        let result = deduplicate(vec![first, older]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].price_usd, Some(0.001));
        assert_eq!(result[0].discovery_labels, vec!["NEW_POOL"]);
    }
    #[test]
    fn recorded_public_responses_replay_without_network() {
        let dex_body: Value = serde_json::from_str(include_str!("fixtures/dex-bsc.json")).unwrap();
        let gecko_body: Value =
            serde_json::from_str(include_str!("fixtures/gecko-solana.json")).unwrap();
        let dex_rows = dex(&dex_body, "bsc", "2026-10-13T07:55:44Z", "RECORDED_FIXTURE").unwrap();
        assert_eq!(dex_rows.len(), 2);
        assert_eq!(
            dex_rows[0].token_address,
            "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c"
        );
        let gecko_rows = gecko(
            &gecko_body,
            "solana",
            "2026-10-13T07:55:44Z",
            "RECORDED_FIXTURE",
            "NEW_POOL",
        )
        .unwrap();
        assert_eq!(gecko_rows.len(), 2);
        assert_eq!(gecko_rows[0].market_cap_usd, None);
        assert_eq!(
            gecko_rows[0].token_address,
            "9t1i1aZ7nMPJ2zav6haP8BDeqAnLdqYXFEsBbTzZpump"
        );
        assert!(gecko(
            &gecko_body,
            "bsc",
            "2026-10-13T07:55:44Z",
            "RECORDED_FIXTURE",
            "NEW_POOL"
        )
        .unwrap()
        .is_empty());
    }
}

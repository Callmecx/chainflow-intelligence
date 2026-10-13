use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    pub min_liquidity_usd: f64,
    pub min_market_cap_usd: f64,
    pub max_market_cap_usd: f64,
    pub max_pool_age_hours: i64,
    pub observation_ttl_seconds: i64,
    pub confirmation_delay_seconds: i64,
    pub cooldown_seconds: i64,
    pub liquidity_drop_pct: f64,
    pub drawdown_pct: f64,
    pub max_sample_gap_seconds: i64,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            min_liquidity_usd: 3000.0,
            min_market_cap_usd: 5000.0,
            max_market_cap_usd: 200000.0,
            max_pool_age_hours: 4,
            observation_ttl_seconds: 7200,
            confirmation_delay_seconds: 360,
            cooldown_seconds: 900,
            liquidity_drop_pct: 30.0,
            drawdown_pct: 20.0,
            max_sample_gap_seconds: 300,
        }
    }
}
impl Rules {
    pub fn from_env() -> anyhow::Result<Self> {
        let rules = match std::env::var("CHAINFLOW_RULES_JSON") {
            Ok(v) => serde_json::from_str(&v)?,
            Err(_) => Self::default(),
        };
        anyhow::ensure!(
            rules.min_liquidity_usd.is_finite()
                && rules.min_liquidity_usd >= 0.0
                && rules.min_market_cap_usd.is_finite()
                && rules.max_market_cap_usd.is_finite()
                && rules.min_market_cap_usd >= 0.0
                && rules.max_market_cap_usd >= rules.min_market_cap_usd,
            "invalid USD thresholds"
        );
        anyhow::ensure!(
            rules.observation_ttl_seconds >= 360
                && rules.confirmation_delay_seconds >= 360
                && rules.cooldown_seconds > 0
                && rules.max_sample_gap_seconds > 0
                && rules.max_pool_age_hours > 0,
            "invalid time thresholds (confirmation must be >=6 minutes)"
        );
        anyhow::ensure!(
            (0.0..=100.0).contains(&rules.liquidity_drop_pct)
                && (0.0..=100.0).contains(&rules.drawdown_pct),
            "invalid risk percentage"
        );
        Ok(rules)
    }
}

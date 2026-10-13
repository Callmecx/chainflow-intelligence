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
    pub outcome_tolerance_seconds: i64,
    pub research_min_samples: usize,
    pub structure_weights: [f64; 4],
    pub breakout_buffer_pct: f64,
    pub recovery_seconds: i64,
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
            outcome_tolerance_seconds: 180,
            research_min_samples: 30,
            structure_weights: [30.0, 25.0, 25.0, 20.0],
            breakout_buffer_pct: 1.0,
            recovery_seconds: 900,
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
        anyhow::ensure!(
            rules.outcome_tolerance_seconds > 0
                && rules.outcome_tolerance_seconds <= 300
                && rules.research_min_samples >= 5
                && rules.recovery_seconds >= 600,
            "invalid research/recovery thresholds"
        );
        anyhow::ensure!(
            rules
                .structure_weights
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0)
                && rules.structure_weights.iter().sum::<f64>() > 0.0
                && rules.breakout_buffer_pct.is_finite()
                && (0.1..=10.0).contains(&rules.breakout_buffer_pct),
            "invalid score weights or breakout buffer"
        );
        Ok(rules)
    }
}

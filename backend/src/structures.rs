//! Approximate structures from timestamped samples, never fabricated OHLCV.
use crate::{config::Rules, models::*};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

pub const RULE_VERSION: &str = "sampled-v1.1.0";

fn window<'a>(
    history: &'a [Point],
    end: DateTime<Utc>,
    seconds: i64,
    resolution: i64,
    source: &str,
) -> Vec<&'a Point> {
    let start = end - chrono::Duration::seconds(seconds);
    let rows: Vec<_> = history
        .iter()
        .filter(|p| {
            timestamp(&p.at)
                .is_some_and(|at| at >= start - chrono::Duration::seconds(resolution) && at <= end)
        })
        .collect();
    if source.is_empty()
        || rows.len() < 4
        || rows
            .iter()
            .any(|p| p.source != source || !p.price_usd.is_some_and(|v| v.is_finite() && v > 0.0))
    {
        return vec![];
    }
    let times: Vec<_> = rows.iter().filter_map(|p| timestamp(&p.at)).collect();
    if times.len() != rows.len()
        || times[0] > start
        || (end - times[times.len() - 1]).num_seconds() > resolution
        || !times.windows(2).all(|v| {
            let gap = (v[1] - v[0]).num_seconds();
            gap > 0 && gap <= resolution
        })
    {
        return vec![];
    }
    rows
}

fn describe(rows: &[&Point]) -> Value {
    if rows.len() < 4 {
        return json!({"status":"INSUFFICIENT_RESOLUTION","kind":"SAMPLED","reason":"同源连续样本、时间覆盖或精度不足；不是原始K线"});
    }
    let prices: Vec<f64> = rows.iter().map(|p| p.price_usd.unwrap()).collect();
    let mid = prices.len() / 2;
    let first_low = prices[..mid].iter().copied().fold(f64::INFINITY, f64::min);
    let second_low = prices[mid..].iter().copied().fold(f64::INFINITY, f64::min);
    let low = prices.iter().copied().fold(f64::INFINITY, f64::min);
    let high = prices.iter().copied().fold(0.0, f64::max);
    let current = *prices.last().unwrap();
    let activity: Vec<_> = rows
        .iter()
        .map(|p| p.buys_5m.zip(p.sells_5m).map(|(b, s)| b.saturating_add(s)))
        .collect();
    let activity_recovery = activity.iter().all(Option::is_some)
        && activity[..mid]
            .iter()
            .flatten()
            .copied()
            .max()
            .is_some_and(|v| v <= 10)
        && activity[mid..]
            .iter()
            .flatten()
            .filter(|v| **v >= 20)
            .count()
            >= 2;
    json!({"status":"AVAILABLE","kind":"SAMPLED","samples":rows.len(),"first_at":rows[0].at,"last_at":rows.last().unwrap().at,"low_usd":low,"high_usd":high,"change_pct":(current/prices[0]-1.0)*100.0,"range_pct":(high/low-1.0)*100.0,"higher_lows":second_low>first_low*1.01,"first_half_low":first_low,"second_half_low":second_low,"activity_recovery":activity_recovery,"activity_metric":"rolling_5m_transaction_count; overlapping windows, not money flow"})
}

pub fn analyze(t: &TokenSnapshot, history: &[Point], rules: &Rules, clock: DateTime<Utc>) -> Value {
    let end = timestamp(&t.fetched_at).unwrap_or(clock);
    let fresh = end <= clock + chrono::Duration::seconds(5)
        && (clock - end).num_seconds() <= rules.max_sample_gap_seconds;
    let w3 = if fresh {
        window(history, end, 180, 60, &t.source)
    } else {
        vec![]
    };
    let w5 = if fresh {
        window(history, end, 300, 150, &t.source)
    } else {
        vec![]
    };
    let w15 = if fresh {
        window(history, end, 900, 300, &t.source)
    } else {
        vec![]
    };
    let d5 = describe(&w5);
    let d15 = describe(&w15);
    // The reference platform ends BEFORE the two newest samples. No future bars.
    let reference = if w15.len() >= 8 {
        &w15[..w15.len() - 2]
    } else {
        &[]
    };
    let ceiling = reference
        .iter()
        .filter_map(|p| p.price_usd)
        .reduce(f64::max);
    let floor = reference
        .iter()
        .filter_map(|p| p.price_usd)
        .reduce(f64::min);
    let compressed = ceiling
        .zip(floor)
        .is_some_and(|(h, l)| l > 0.0 && h / l <= 1.08);
    let breakout = compressed
        && ceiling
            .zip(t.price_usd)
            .is_some_and(|(h, p)| p >= h * (1.0 + rules.breakout_buffer_pct / 100.0));
    let higher = d5["higher_lows"] == true;
    let multi = higher
        && d15["higher_lows"] == true
        && d5["change_pct"].as_f64().is_some_and(|v| v >= 2.0)
        && d15["change_pct"].as_f64().is_some_and(|v| v >= 3.0);
    let liquidity_stable = w5.len() >= 4
        && w5
            .first()
            .and_then(|p| p.liquidity_usd)
            .zip(t.liquidity_usd)
            .is_some_and(|(a, b)| a > 0.0 && b >= a * 0.95);
    let activity = d5["activity_recovery"] == true;
    let flags = [higher, multi, breakout, liquidity_stable];
    let names = [
        "5m低点抬高",
        "5m/15m同向结构",
        "压缩平台突破",
        "5m流动性维持",
    ];
    let weights = rules.structure_weights;
    let total: f64 = weights.iter().sum();
    let factors:Vec<_>=flags.iter().enumerate().map(|(i,matched)|json!({"name":names[i],"value":if *matched {1.0}else{0.0},"weight":weights[i],"score":if *matched {weights[i]/total*100.0}else{0.0},"evidence":[format!("matched={matched}; source={}; at={}",t.source,t.fetched_at)]})).collect();
    json!({"kind":"SAMPLED","rule_version":RULE_VERSION,"windows":{"3m":describe(&w3),"5m":d5,"15m":d15},"score":factors.iter().filter_map(|v|v["score"].as_f64()).sum::<f64>(),"score_label":"可解释条件覆盖分，非上涨概率","factors":factors,"multi_period":multi,"breakout":breakout,"platform_high":ceiling,"platform_low":floor,"activity_recovery":activity,"limitations":["默认120秒轮询不能支持3m细粒度K线识别","同源SAMPLED点仅表示采样时刻，滚动成交笔数不等于独立成交量","采样缺口、源切换或缺价格时不确认结构"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn points(start: DateTime<Utc>) -> Vec<Point> {
        (0..=10)
            .map(|i| Point {
                at: (start + chrono::Duration::seconds(i * 120)).to_rfc3339(),
                price_usd: Some(1.0 + i as f64 * 0.01),
                source: "fixture".into(),
                ..Default::default()
            })
            .collect()
    }
    #[test]
    fn resolution_gaps_sources_and_future_are_explicit() {
        let start = Utc::now();
        let mut h = points(start);
        let end = start + chrono::Duration::seconds(1200);
        let t = TokenSnapshot {
            source: "fixture".into(),
            fetched_at: end.to_rfc3339(),
            price_usd: Some(1.1),
            ..Default::default()
        };
        let a = analyze(&t, &h, &Rules::default(), end);
        assert_eq!(a["windows"]["3m"]["status"], "INSUFFICIENT_RESOLUTION");
        assert_eq!(a["windows"]["15m"]["status"], "AVAILABLE");
        h[8].source = "different".into();
        assert_eq!(
            analyze(&t, &h, &Rules::default(), end)["windows"]["5m"]["status"],
            "INSUFFICIENT_RESOLUTION"
        );
        assert_eq!(
            analyze(&t, &points(start), &Rules::default(), start)["multi_period"],
            false
        );
        h = points(start);
        h.drain(5..8);
        assert_eq!(
            analyze(&t, &h, &Rules::default(), end)["multi_period"],
            false
        );
    }
}

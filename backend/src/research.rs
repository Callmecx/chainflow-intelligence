//! Immutable signal anchors and strictly observed, explicitly censored outcomes.
use crate::{config::Rules, models::*};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub signal_id: String,
    pub chain: String,
    pub token_address: String,
    pub pool_address: String,
    pub rule: String,
    pub rule_version: String,
    pub trigger_at: String,
    pub source_at: String,
    pub baseline_price_usd: Option<f64>,
    pub baseline_liquidity_usd: Option<f64>,
    pub source: String,
    pub factors: Value,
    pub evidence_snapshot: Value,
    pub windows: Value,
    pub max_gap_seconds: i64,
    pub tolerance_seconds: i64,
}
pub fn init_schema(c: &Connection) -> anyhow::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS signal_research(signal_id TEXT PRIMARY KEY, pool_key TEXT NOT NULL, trigger_at TEXT NOT NULL, settled INTEGER NOT NULL DEFAULT 0, json TEXT NOT NULL); CREATE INDEX IF NOT EXISTS research_pending ON signal_research(settled, trigger_at);")?;
    Ok(())
}
pub fn register(
    c: &Connection,
    s: &Signal,
    t: &TokenSnapshot,
    rules: &Rules,
) -> anyhow::Result<()> {
    if s.severity != "CONFIRMATION" || s.state != "CONFIRMED" {
        return Ok(());
    }
    let r = Record {
        signal_id: s.id.clone(),
        chain: s.chain.clone(),
        token_address: s.token_address.clone(),
        pool_address: s.pool_address.clone(),
        rule: s.rule.clone(),
        rule_version: s.rule_version.clone(),
        trigger_at: s.at.clone(),
        source_at: t.fetched_at.clone(),
        baseline_price_usd: t.price_usd,
        baseline_liquidity_usd: t.liquidity_usd,
        source: t.source.clone(),
        factors: s.factors.clone(),
        evidence_snapshot: s.evidence_snapshot.clone(),
        windows: json!({}),
        max_gap_seconds: rules.max_sample_gap_seconds,
        tolerance_seconds: rules.outcome_tolerance_seconds,
    };
    c.execute("INSERT OR IGNORE INTO signal_research(signal_id,pool_key,trigger_at,json) VALUES(?1,?2,?3,?4)",params![s.id,t.key(),s.at,serde_json::to_string(&r)?])?;
    Ok(())
}
fn pct(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    a.zip(b)
        .filter(|(a, b)| a.is_finite() && *a > 1e-12 && b.is_finite() && *b >= 0.0)
        .map(|(a, b)| (b / a - 1.0) * 100.0)
}

pub fn outcomes(
    r: &Record,
    points: &[Point],
    invalidations: &[String],
    clock: DateTime<Utc>,
) -> Value {
    let Some(start) = timestamp(&r.trigger_at) else {
        return json!({});
    };
    let mut windows = Map::new();
    for (name, seconds) in [("1h", 3600), ("2h", 7200), ("6h", 21600)] {
        let target = start + chrono::Duration::seconds(seconds);
        let deadline = target + chrono::Duration::seconds(r.tolerance_seconds);
        let mut rows: Vec<_> = points
            .iter()
            .filter(|p| {
                timestamp(&p.at).is_some_and(|at| at >= start && at <= deadline && at <= clock)
            })
            .collect();
        rows.sort_by_key(|p| timestamp(&p.at));
        rows.dedup_by_key(|p| timestamp(&p.at));
        let endpoint = rows
            .iter()
            .filter(|p| {
                timestamp(&p.at)
                    .is_some_and(|at| (at - target).num_seconds().abs() <= r.tolerance_seconds)
                    && p.price_usd.is_some_and(|p| p > 0.0)
            })
            .min_by_key(|p| {
                (
                    (timestamp(&p.at).unwrap() - target).num_seconds().abs(),
                    timestamp(&p.at).unwrap(),
                )
            });
        let observed = endpoint.and_then(|p| timestamp(&p.at));
        let measured: Vec<_> = rows
            .iter()
            .copied()
            .filter(|p| {
                timestamp(&p.at).is_some_and(|at| at <= observed.unwrap_or(clock.min(target)))
            })
            .collect();
        let mut prev = start;
        let mut gap = 0i64;
        let mut source_change = false;
        let mut missing_price = false;
        for p in &measured {
            let at = timestamp(&p.at).unwrap();
            gap = gap.max((at - prev).num_seconds());
            prev = at;
            source_change |= p.source != r.source;
            missing_price |= !p.price_usd.is_some_and(|v| v.is_finite() && v > 0.0);
        }
        let invalidated = invalidations
            .iter()
            .filter_map(|s| timestamp(s))
            .any(|at| at >= start && at <= target && at <= clock);
        let (status, reason) = if clock < deadline {
            ("PENDING", "目标时间及容差尚未到达，禁止填未来数据")
        } else if endpoint.is_none() {
            ("MISSING", "目标时点±容差内没有有效行情，不按归零处理")
        } else if r.baseline_price_usd.is_none_or(|p| p <= 1e-12)
            || gap > r.max_gap_seconds
            || source_change
            || missing_price
        {
            (
                "CENSORED",
                "样本缺口、来源切换或无效价格，保留分母并拒绝完整收益统计",
            )
        } else {
            ("COMPLETE", "仅实际同源SAMPLED点，非采样间隔内真实极值")
        };
        let complete = status == "COMPLETE";
        let returns: Vec<_> = measured
            .iter()
            .filter_map(|p| pct(r.baseline_price_usd, p.price_usd))
            .collect();
        let mut peak = r.baseline_price_usd.unwrap_or(0.0);
        let mut drawdown = 0.0f64;
        for p in measured.iter().filter_map(|p| p.price_usd) {
            peak = peak.max(p);
            if peak > 0.0 {
                drawdown = drawdown.min((p / peak - 1.0) * 100.0)
            }
        }
        windows.insert(name.into(),json!({"status":status,"samples":measured.len(),"return_pct":if complete{endpoint.and_then(|p|pct(r.baseline_price_usd,p.price_usd))}else{None},"max_positive_pct":if complete{Some(returns.iter().copied().fold(0.0,f64::max))}else{None},"max_negative_pct":if complete{Some(returns.iter().copied().fold(0.0,f64::min))}else{None},"max_drawdown_pct":if complete{Some(drawdown)}else{None},"liquidity_change_pct":if complete{endpoint.and_then(|p|pct(r.baseline_liquidity_usd,p.liquidity_usd))}else{None},"invalidated":invalidated,"target_at":target.to_rfc3339(),"observed_at":observed.map(|at|at.to_rfc3339()),"max_gap_seconds":gap,"tolerance_seconds":r.tolerance_seconds,"source_changed":source_change,"reason":reason}));
    }
    Value::Object(windows)
}

pub fn refresh(c: &Connection, clock: DateTime<Utc>) -> anyhow::Result<()> {
    let pending = {
        let mut stmt = c.prepare("SELECT pool_key,json FROM signal_research WHERE settled=0")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (key, data) in pending {
        let mut r: Record = serde_json::from_str(&data)?;
        let end = (timestamp(&r.trigger_at).unwrap()
            + chrono::Duration::seconds(21600 + r.tolerance_seconds))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut stmt =
            c.prepare("SELECT json FROM samples WHERE key=?1 AND at>=?2 AND at<=?3 ORDER BY at")?;
        let mut points = Vec::new();
        for row in stmt.query_map(params![key, r.trigger_at, end], |row| {
            row.get::<_, String>(0)
        })? {
            let t: TokenSnapshot = serde_json::from_str(&row?)?;
            points.push(Point::from(&t));
        }
        let mut stmt = c.prepare("SELECT json FROM signals WHERE at>=?1 AND at<=?2")?;
        let mut invalidations = Vec::new();
        for row in stmt.query_map(params![r.trigger_at, end], |row| row.get::<_, String>(0))? {
            let s: Signal = serde_json::from_str(&row?)?;
            if s.chain == r.chain
                && s.token_address == r.token_address
                && s.pool_address == r.pool_address
                && s.state == "INVALIDATED"
            {
                invalidations.push(s.at);
            }
        }
        let fresh = outcomes(&r, &points, &invalidations, clock);
        for label in ["1h", "2h", "6h"] {
            if r.windows[label].is_null() || r.windows[label]["status"] == "PENDING" {
                r.windows[label] = fresh[label].clone();
            }
        }
        let settled = ["1h", "2h", "6h"]
            .iter()
            .all(|k| r.windows[k]["status"] != "PENDING");
        c.execute(
            "UPDATE signal_research SET json=?1,settled=?2 WHERE signal_id=?3",
            params![serde_json::to_string(&r)?, settled, r.signal_id],
        )?;
    }
    Ok(())
}

pub fn report(c: &Connection, min_samples: usize) -> anyhow::Result<Value> {
    let mut stmt = c.prepare("SELECT json FROM signal_research ORDER BY trigger_at DESC")?;
    let mut records = Vec::<Record>::new();
    for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
        records.push(serde_json::from_str(&row?)?);
    }
    let mut groups: BTreeMap<(String, String), Vec<&Record>> = BTreeMap::new();
    for r in &records {
        groups
            .entry((r.rule.clone(), r.rule_version.clone()))
            .or_default()
            .push(r);
    }
    let mut stats = Vec::new();
    for ((rule, version), rows) in groups {
        for label in ["1h", "2h", "6h"] {
            let mut values: Vec<f64> = rows
                .iter()
                .filter(|r| r.windows[label]["status"] == "COMPLETE")
                .filter_map(|r| r.windows[label]["return_pct"].as_f64())
                .collect();
            values.sort_by(f64::total_cmp);
            let n = values.len();
            let mean = if n > 0 {
                Some(values.iter().sum::<f64>() / n as f64)
            } else {
                None
            };
            let median = if n == 0 {
                None
            } else if n.is_multiple_of(2) {
                Some((values[n / 2 - 1] + values[n / 2]) / 2.0)
            } else {
                Some(values[n / 2])
            };
            let count = |s: &str| {
                rows.iter()
                    .filter(|r| r.windows[label]["status"] == s)
                    .count()
            };
            let missing = count("MISSING");
            let censored = count("CENSORED");
            let pending = count("PENDING");
            let eligible = rows.len() - pending;
            stats.push(json!({"rule":rule,"rule_version":version,"window":label,"trigger_count":rows.len(),"eligible_count":eligible,"valid_count":n,"missing_count":missing,"censored_count":censored,"pending_count":pending,"missing_rate":if eligible>0{Some((missing+censored)as f64/eligible as f64)}else{None},"mean_return_pct":mean,"median_return_pct":median,"distribution":{"negative":values.iter().filter(|v|**v< -0.1).count(),"flat":values.iter().filter(|v|v.abs()<=0.1).count(),"positive":values.iter().filter(|v|**v>0.1).count()},"status":if n<min_samples{"INSUFFICIENT_SAMPLE"}else{"DESCRIPTIVE_ONLY"}}));
        }
    }
    let complete = stats.iter().any(|s| s["status"] == "DESCRIPTIVE_ONLY");
    Ok(
        json!({"generated_at_utc":now(),"status":if complete{"DESCRIPTIVE_ONLY"}else{"INSUFFICIENT_SAMPLE"},"min_sample_size":min_samples,"total_records":records.len(),"records":records,"rule_stats":stats,"limitations":["基于正式信号的前瞻SAMPLED记录；不回填旧版信号","MISSING/CENSORED保留统计分母，缺行情不归零","费用、滑点和采样间的极值均未建模；不宣称盈利优势","窗口在目标时间+容差后首次评估冻结；数据断档会明确删失"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggregate_preserves_missing_denominators_and_median() {
        let c = Connection::open_in_memory().unwrap();
        init_schema(&c).unwrap();
        for (i, status) in ["COMPLETE", "COMPLETE", "MISSING", "CENSORED", "PENDING"]
            .iter()
            .enumerate()
        {
            let mut r = record(Utc::now());
            r.signal_id = format!("TEST_{i}");
            r.windows = json!({"1h":{"status":status,"return_pct":if *status=="COMPLETE"{Some((i+1)as f64*10.0)}else{None}},"2h":{"status":"PENDING"},"6h":{"status":"PENDING"}});
            c.execute("INSERT INTO signal_research(signal_id,pool_key,trigger_at,json)VALUES(?1,'test',?2,?3)",params![r.signal_id,r.trigger_at,serde_json::to_string(&r).unwrap()]).unwrap();
        }
        let report = report(&c, 30).unwrap();
        let stat = report["rule_stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["window"] == "1h")
            .unwrap();
        assert_eq!(stat["trigger_count"], 5);
        assert_eq!(stat["eligible_count"], 4);
        assert_eq!(stat["valid_count"], 2);
        assert_eq!(stat["missing_rate"], 0.5);
        assert_eq!(stat["median_return_pct"], 15.0);
        assert_eq!(stat["mean_return_pct"], 15.0);
        assert_eq!(stat["status"], "INSUFFICIENT_SAMPLE");
    }
    #[test]
    fn settled_results_do_not_rewrite_after_late_replay() {
        let c = Connection::open_in_memory().unwrap();
        init_schema(&c).unwrap();
        c.execute_batch("CREATE TABLE samples(key TEXT,at TEXT,json TEXT);CREATE TABLE signals(json TEXT,at TEXT);").unwrap();
        let start = Utc::now() - chrono::Duration::hours(8);
        let mut r = record(start);
        r.trigger_at = start.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        c.execute("INSERT INTO signal_research(signal_id,pool_key,trigger_at,json)VALUES('test','pool',?1,?2)",params![r.trigger_at,serde_json::to_string(&r).unwrap()]).unwrap();
        refresh(&c, Utc::now()).unwrap();
        let frozen: String = c
            .query_row("SELECT json FROM signal_research", [], |r| r.get(0))
            .unwrap();
        let t = TokenSnapshot {
            fetched_at: (start + chrono::Duration::hours(1))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            price_usd: Some(10.0),
            source: "fixture".into(),
            ..Default::default()
        };
        c.execute(
            "INSERT INTO samples VALUES('pool',?1,?2)",
            params![t.fetched_at, serde_json::to_string(&t).unwrap()],
        )
        .unwrap();
        refresh(&c, Utc::now()).unwrap();
        assert_eq!(
            c.query_row("SELECT json FROM signal_research", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            frozen
        );
        assert_eq!(
            serde_json::from_str::<Value>(&frozen).unwrap()["windows"]["1h"]["status"],
            "MISSING"
        );
    }
    fn record(start: DateTime<Utc>) -> Record {
        Record {
            signal_id: "test".into(),
            chain: "solana".into(),
            token_address: "mint".into(),
            pool_address: "pool".into(),
            rule: "TEST".into(),
            rule_version: "fixture".into(),
            trigger_at: start.to_rfc3339(),
            source_at: start.to_rfc3339(),
            baseline_price_usd: Some(1.0),
            baseline_liquidity_usd: Some(1000.0),
            source: "fixture".into(),
            factors: json!([]),
            evidence_snapshot: json!({}),
            windows: json!({}),
            max_gap_seconds: 300,
            tolerance_seconds: 180,
        }
    }
    fn points(start: DateTime<Utc>, hours: i64) -> Vec<Point> {
        (1..=hours * 30)
            .map(|i| Point {
                at: (start + chrono::Duration::seconds(i * 120)).to_rfc3339(),
                price_usd: Some(1.0 + i as f64 * 0.01),
                liquidity_usd: Some(1100.0),
                source: "fixture".into(),
                ..Default::default()
            })
            .collect()
    }
    #[test]
    fn future_data_never_leaks_and_horizons_are_correct() {
        let t = Utc::now();
        let r = record(t);
        let p = points(t, 6);
        let early = outcomes(&r, &p, &[], t + chrono::Duration::minutes(10));
        assert_eq!(early["1h"]["status"], "PENDING");
        assert!(early["1h"]["return_pct"].is_null());
        let full = outcomes(&r, &p, &[], t + chrono::Duration::hours(7));
        for (label, expected) in [("1h", 30.0), ("2h", 60.0), ("6h", 180.0)] {
            assert_eq!(full[label]["status"], "COMPLETE");
            assert!((full[label]["return_pct"].as_f64().unwrap() - expected).abs() < 1e-6);
            assert_eq!(
                full[label]["liquidity_change_pct"]
                    .as_f64()
                    .unwrap()
                    .round(),
                10.0
            );
        }
    }
    #[test]
    fn missing_endpoint_gap_and_source_switch_not_silently_dropped() {
        let t = Utc::now();
        let r = record(t);
        let mut p = points(t, 1);
        let clock = t + chrono::Duration::hours(7);
        p.retain(|p| timestamp(&p.at).unwrap() < t + chrono::Duration::minutes(55));
        assert_eq!(outcomes(&r, &p, &[], clock)["1h"]["status"], "MISSING");
        let mut p = points(t, 1);
        p.drain(4..12);
        assert_eq!(outcomes(&r, &p, &[], clock)["1h"]["status"], "CENSORED");
        let mut p = points(t, 1);
        p[5].source = "second".into();
        assert_eq!(outcomes(&r, &p, &[], clock)["1h"]["status"], "CENSORED");
    }
    #[test]
    fn drawdown_is_peak_to_trough_not_just_baseline_loss() {
        let t = Utc::now();
        let r = record(t);
        let mut p = points(t, 1);
        p[5].price_usd = Some(2.0);
        p[6].price_usd = Some(1.0);
        let out = outcomes(
            &r,
            &p,
            &[(t + chrono::Duration::minutes(15)).to_rfc3339()],
            t + chrono::Duration::hours(2),
        );
        assert_eq!(out["1h"]["max_drawdown_pct"], -50.0);
        assert_eq!(out["1h"]["invalidated"], true);
    }
}

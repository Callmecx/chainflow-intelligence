use crate::{config::Rules, models::*};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

fn change(old: Option<f64>, new: Option<f64>) -> Option<f64> {
    match (old, new) {
        (Some(a), Some(b)) if a > 1e-12 && a.is_finite() && b.is_finite() => {
            Some((b / a - 1.0) * 100.0)
        }
        _ => None,
    }
}
fn signal(
    t: &TokenSnapshot,
    rule: &str,
    severity: &str,
    state: &str,
    message: &str,
    evidence: Vec<String>,
) -> Signal {
    Signal {
        id: format!("{}:{}:{}", t.key(), rule, t.fetched_at),
        chain: t.chain.clone(),
        token_address: t.token_address.clone(),
        pool_address: t.pool_address.clone(),
        rule: rule.into(),
        severity: severity.into(),
        at: t.fetched_at.clone(),
        state: state.into(),
        message: message.into(),
        evidence,
    }
}
fn enough(points: &[Point], seconds: i64, max_gap: i64) -> bool {
    if points.len() < 4 {
        return false;
    }
    let times: Vec<_> = points.iter().filter_map(|p| timestamp(&p.at)).collect();
    times.len() == points.len()
        && (times[times.len() - 1] - times[0]).num_seconds() >= seconds
        && times.windows(2).all(|w| {
            let gap = (w[1] - w[0]).num_seconds();
            gap > 0 && gap <= max_gap
        })
        && points.iter().all(|p| p.price_usd.is_some_and(|v| v > 0.0))
}
pub fn outcome(history: &[Point], max_gap: i64) -> Value {
    let mut windows = Map::new();
    for (label, seconds) in [("1h", 3600), ("2h", 7200), ("6h", 21600)] {
        let result = if let (Some(first), Some(last)) = (history.first(), history.last()) {
            let start = timestamp(&first.at).unwrap_or_default();
            let end = start + chrono::Duration::seconds(seconds);
            let selected: Vec<_> = history
                .iter()
                .filter(|p| timestamp(&p.at).is_some_and(|at| at <= end))
                .cloned()
                .collect();
            let covered = timestamp(&last.at).is_some_and(|at| at >= end);
            let reaches_end = selected
                .last()
                .and_then(|p| timestamp(&p.at))
                .is_some_and(|at| (end - at).num_seconds() <= max_gap);
            if covered && reaches_end && enough(&selected, seconds - max_gap, max_gap) {
                let changes: Vec<_> = selected
                    .iter()
                    .filter_map(|p| change(first.price_usd, p.price_usd))
                    .collect();
                json!({"status":"COMPLETE","samples":selected.len(),"max_positive_pct":changes.iter().copied().fold(0.0,f64::max),"max_negative_pct":changes.iter().copied().fold(0.0,f64::min),"coverage_seconds":seconds,"reason":"仅真实 SAMPLED 采样范围，非连续成交极值"})
            } else {
                json!({"status":"UNKNOWN","samples":selected.len(),"max_positive_pct":null,"max_negative_pct":null,"coverage_seconds":(timestamp(&last.at).unwrap_or(start)-start).num_seconds().min(seconds),"reason":"未来区间尚未完成、价格缺失或采样间隔过大"})
            }
        } else {
            json!({"status":"UNKNOWN","samples":0,"max_positive_pct":null,"max_negative_pct":null,"coverage_seconds":0,"reason":"无真实采样"})
        };
        windows.insert(label.into(), result);
    }
    json!({"kind":"SAMPLED_OBSERVATION","anchor_at":history.first().map(|p|p.at.clone()),"baseline":"first_observed_price","windows":windows})
}

pub fn evaluate(
    t: TokenSnapshot,
    previous: Option<&TokenView>,
    history: Vec<Point>,
    rules: &Rules,
    clock: DateTime<Utc>,
) -> (TokenView, Vec<Signal>) {
    let first = previous
        .map(|p| p.first_seen_at.clone())
        .unwrap_or_else(|| t.fetched_at.clone());
    let age = timestamp(&first)
        .map(|at| (clock - at).num_seconds())
        .unwrap_or(0);
    let mut state = previous
        .map(|p| p.state.clone())
        .unwrap_or_else(|| "DISCOVERED".into());
    let mut label = "NEW".to_string();
    let mut why = Vec::new();
    let mut evidence = Vec::new();
    let mut incomplete = vec!["HONEYPOT / TOP10 / SMART_MONEY = UNKNOWN；未独立验证".into()];
    let mut emitted = Vec::new();
    let fresh = timestamp(&t.fetched_at)
        .is_some_and(|at| (clock - at).num_seconds().abs() <= rules.max_sample_gap_seconds);
    if !fresh {
        incomplete.push("STALE：数据过期，不升级状态".into());
    }
    if t.market_cap_usd.is_none() {
        incomplete.push("市值 UNKNOWN；FDV 不替代市值，不能满足市值筛选".into());
    }
    if t.pool_created_at.is_none() {
        incomplete.push("池创建时间 UNKNOWN，不能确认新池年龄".into());
    }
    let eligible = t
        .liquidity_usd
        .is_some_and(|v| v >= rules.min_liquidity_usd)
        && t.market_cap_usd
            .is_some_and(|v| v >= rules.min_market_cap_usd && v <= rules.max_market_cap_usd)
        && t.pool_created_at
            .as_deref()
            .and_then(timestamp)
            .is_some_and(|v| {
                let h = (clock - v).num_seconds();
                h >= 0 && h <= rules.max_pool_age_hours * 3600
            });
    let recent: Vec<_> = history
        .iter()
        .filter(|p| {
            timestamp(&p.at)
                .is_some_and(|at| (clock - at).num_seconds() <= rules.max_sample_gap_seconds + 300)
        })
        .cloned()
        .collect();
    let sufficient = enough(&recent, 300, rules.max_sample_gap_seconds);
    for (label, secs) in [("5m", 300), ("15m", 900), ("1h", 3600)] {
        if !enough(&history, secs, rules.max_sample_gap_seconds) {
            incomplete.push(format!("{label}结构：真实连续采样不足"));
        }
    }
    let mut risk = false;
    if fresh {
        if let Some(prev) = previous {
            let gap = timestamp(&t.fetched_at)
                .zip(timestamp(&prev.token.fetched_at))
                .map(|(a, b)| (a - b).num_seconds())
                .unwrap_or(i64::MAX);
            if gap > 0 && gap <= rules.max_sample_gap_seconds {
                if let Some(pct) = change(prev.token.liquidity_usd, t.liquidity_usd)
                    .filter(|pct| *pct <= -rules.liquidity_drop_pct)
                {
                    risk = true;
                    emitted.push(signal(
                        &t,
                        "LIQUIDITY_DROP",
                        "RISK",
                        "INVALIDATED",
                        "相邻真实样本流动性急跌",
                        vec![format!("liquidity_change={pct:.2}%; interval={gap}s")],
                    ));
                }
                if let Some(pct) = change(prev.token.price_usd, t.price_usd)
                    .filter(|pct| *pct <= -rules.drawdown_pct)
                {
                    risk = true;
                    emitted.push(signal(
                        &t,
                        "DRAWDOWN",
                        "RISK",
                        "INVALIDATED",
                        "相邻真实样本价格大幅回撤",
                        vec![format!("price_change={pct:.2}%; interval={gap}s")],
                    ));
                }
                if prev
                    .token
                    .volume_24h_usd
                    .zip(t.volume_24h_usd)
                    .is_some_and(|(a, b)| a > 0.0 && b > a * 1.5)
                    && change(prev.token.price_usd, t.price_usd).is_some_and(|v| v < -5.0)
                {
                    risk = true;
                    emitted.push(signal(
                        &t,
                        "VOLUME_PRICE_DIVERGENCE",
                        "RISK",
                        "INVALIDATED",
                        "滚动24h成交量上升伴随价格回落（非独立短时成交量）",
                        vec![format!(
                            "rolling_volume_24h={:?} -> {:?}",
                            prev.token.volume_24h_usd, t.volume_24h_usd
                        )],
                    ));
                }
            } else {
                incomplete.push("相邻采样间隔过大，未推断短时变化".into());
            }
        }
        if let (Some(buys), Some(sells)) = (t.buys_5m, t.sells_5m) {
            let total = buys.saturating_add(sells);
            if total >= 20 && sells as f64 / total as f64 >= 0.85 {
                risk = true;
                emitted.push(signal(
                    &t,
                    "SELL_COUNT_IMBALANCE",
                    "RISK",
                    "INVALIDATED",
                    "卖出笔数占比偏高，笔数不代表净资金流",
                    vec![format!("5m buys={buys}, sells={sells}")],
                ));
            }
        }
    }
    if previous.is_none() {
        emitted.push(signal(
            &t,
            "DISCOVERED",
            "WATCH",
            "DISCOVERED",
            "首次获取公开行情，开始记录观察",
            vec![t.source_url.clone()],
        ));
    }
    let old_state = state.clone();
    if risk {
        state = "INVALIDATED".into();
        label = "RISK".into();
        why.push("触发风险规则，停止确认升级".into());
    } else if age >= rules.observation_ttl_seconds {
        state = "EXPIRED".into();
        label = "EXPIRED".into();
        why.push("观察TTL已到，仍保留行情采样及结果研究".into());
    } else if state == "INVALIDATED" {
        label = "RISK".into();
        why.push("先前风险已使本轮观察失效，不自动重新确认".into());
    } else if fresh && previous.is_some() {
        if eligible {
            let delta = recent
                .first()
                .and_then(|p| change(p.price_usd, t.price_usd));
            why.push("满足配置的新池年龄、市值、流动性观察条件".into());
            if sufficient
                && age >= rules.confirmation_delay_seconds
                && delta.is_some_and(|v| v >= 5.0)
            {
                label = "MOMENTUM_WATCH".into();
                state = if old_state == "CONFIRMATION_CANDIDATE" || old_state == "CONFIRMED" {
                    "CONFIRMED"
                } else {
                    "CONFIRMATION_CANDIDATE"
                }
                .into();
                evidence.push(format!(
                    "SAMPLED窗口价格变化={:.2}%; samples={}",
                    delta.unwrap_or(0.0),
                    recent.len()
                ));
            } else {
                state = "WATCH".into();
                label = "ACTIVE".into();
                if sufficient && delta.is_some_and(|v| v.abs() <= 2.0) {
                    label = "RANGE_WATCH".into();
                }
                if sufficient
                    && recent
                        .iter()
                        .filter_map(|p| p.price_usd)
                        .reduce(f64::min)
                        .zip(t.price_usd)
                        .is_some_and(|(lo, v)| lo > 0.0 && v / lo >= 1.05)
                    && delta.is_some_and(|v| v < 5.0)
                {
                    label = "REVIVAL_WATCH".into();
                }
            }
        } else {
            state = "WATCH".into();
            label = "ACTIVE".into();
            why.push("保留行情观察；未满足完整候选阈值，不升级确认".into());
        }
    } else {
        why.push("记录首个样本，等待后续真实采样".into());
    }
    if state != old_state {
        emitted.push(signal(&t,&format!("STATE_{state}"),if state=="CONFIRMED" {"CONFIRMATION"} else {"WATCH"},&state,"规则观察状态变化，不构成投资建议",vec![format!("{old_state} -> {state}; observation_age={age}s; eligible={eligible}; sufficient_samples={sufficient}")]));
    }
    evidence.push(format!(
        "source={}; fetched_at={}; pool={}",
        t.source, t.fetched_at, t.pool_address
    ));
    if t.discovery_labels.iter().any(|v| v == "BOOSTED") {
        why.push("BOOSTED：付费推广收录，不能视为有机热度".into());
    }
    let trigger = previous
        .into_iter()
        .flat_map(|p| p.timeline.iter())
        .chain(emitted.iter())
        .find(|s| {
            s.rule == "STATE_WATCH"
                || s.rule == "STATE_CONFIRMATION_CANDIDATE"
                || s.rule == "STATE_CONFIRMED"
        });
    let triggered_history: Vec<_> = history
        .iter()
        .filter(|p| trigger.is_none_or(|s| timestamp(&p.at) >= timestamp(&s.at)))
        .cloned()
        .collect();
    let mut result = outcome(&triggered_history, rules.max_sample_gap_seconds);
    result["trigger_rule"] = serde_json::json!(trigger.map(|s| s.rule.clone()));
    result["baseline_from_first_observation"] = outcome(&history, rules.max_sample_gap_seconds);
    let timeline = previous.map(|p| p.timeline.clone()).unwrap_or_default();
    (
        TokenView {
            token: t,
            label,
            state,
            first_seen_at: first,
            why,
            evidence,
            incomplete_data: incomplete,
            invalidation: vec![
                format!(
                    "流动性下降≥{}% 或价格回撤≥{}%",
                    rules.liquidity_drop_pct, rules.drawdown_pct
                ),
                format!("TTL={}s；数据失联不得升级", rules.observation_ttl_seconds),
            ],
            history,
            timeline,
            outcome: result,
        },
        emitted,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn token(at: DateTime<Utc>, price: f64, liq: f64) -> TokenSnapshot {
        TokenSnapshot {
            chain: "solana".into(),
            token_address: "So11111111111111111111111111111111111111112".into(),
            pool_address: "pool".into(),
            fetched_at: at.to_rfc3339(),
            pool_created_at: Some((at - chrono::Duration::hours(1)).to_rfc3339()),
            price_usd: Some(price),
            liquidity_usd: Some(liq),
            market_cap_usd: Some(10000.0),
            ..Default::default()
        }
    }
    #[test]
    fn confirmation_requires_six_minutes_and_a_second_observation() {
        let start = Utc::now();
        let rules = Rules::default();
        let (prior, _) = evaluate(token(start, 1.0, 10000.0), None, vec![], &rules, start);
        let history: Vec<_> = (0..=3)
            .map(|i| Point {
                at: (start + chrono::Duration::seconds(i * 120)).to_rfc3339(),
                price_usd: Some(1.0 + i as f64 * 0.03),
                liquidity_usd: Some(10000.0),
                volume_24h_usd: None,
            })
            .collect();
        let early = start + chrono::Duration::seconds(240);
        let (view, _) = evaluate(
            token(early, 1.06, 10000.0),
            Some(&prior),
            history[..3].to_vec(),
            &rules,
            early,
        );
        assert_eq!(view.state, "WATCH");
        let at = start + chrono::Duration::seconds(360);
        let (candidate, _) = evaluate(
            token(at, 1.09, 10000.0),
            Some(&prior),
            history.clone(),
            &rules,
            at,
        );
        assert_eq!(candidate.state, "CONFIRMATION_CANDIDATE");
        let later = start + chrono::Duration::seconds(480);
        let mut history = history;
        history.push(Point {
            at: later.to_rfc3339(),
            price_usd: Some(1.12),
            liquidity_usd: Some(10000.0),
            volume_24h_usd: None,
        });
        let (confirmed, events) = evaluate(
            token(later, 1.12, 10000.0),
            Some(&candidate),
            history,
            &rules,
            later,
        );
        assert_eq!(confirmed.state, "CONFIRMED");
        assert!(events.iter().any(|s| s.severity == "CONFIRMATION"));
    }
    #[test]
    fn risk_is_immediate_and_zero_liquidity_safe() {
        let clock = Utc::now();
        let (prior, _) = evaluate(
            token(clock, 1.0, 10000.0),
            None,
            vec![],
            &Rules::default(),
            clock,
        );
        let later = clock + chrono::Duration::seconds(120);
        let (view, events) = evaluate(
            token(later, 0.5, 0.0),
            Some(&prior),
            vec![],
            &Rules::default(),
            later,
        );
        assert_eq!(view.state, "INVALIDATED");
        assert!(events.iter().any(|s| s.rule == "LIQUIDITY_DROP"));
        assert_eq!(change(Some(0.0), Some(1.0)), None);
    }
    #[test]
    fn missing_market_cap_and_insufficient_samples_never_confirm() {
        let clock = Utc::now();
        let (prior, _) = evaluate(
            token(clock, 1.0, 10000.0),
            None,
            vec![],
            &Rules::default(),
            clock,
        );
        let later = clock + chrono::Duration::seconds(600);
        let mut t = token(later, 2.0, 10000.0);
        t.market_cap_usd = None;
        let (view, _) = evaluate(t, Some(&prior), vec![], &Rules::default(), later);
        assert_eq!(view.state, "WATCH");
        assert!(view
            .incomplete_data
            .iter()
            .any(|v| v.contains("市值 UNKNOWN")));
    }
    #[test]
    fn outcomes_require_completed_contiguous_window() {
        let start = Utc::now();
        let points: Vec<_> = (0..=30)
            .map(|i| Point {
                at: (start + chrono::Duration::seconds(i * 120)).to_rfc3339(),
                price_usd: Some(1.0 + i as f64 / 100.0),
                liquidity_usd: None,
                volume_24h_usd: None,
            })
            .collect();
        assert_eq!(
            outcome(&points[..10], 300)["windows"]["1h"]["status"],
            "UNKNOWN"
        );
        assert_eq!(outcome(&points, 300)["windows"]["1h"]["status"], "COMPLETE");
        let mut broken = points.clone();
        broken.drain(3..15);
        assert_eq!(outcome(&broken, 300)["windows"]["1h"]["status"], "UNKNOWN");
    }
    #[test]
    fn stale_cannot_upgrade_and_ttl_expires() {
        let clock = Utc::now();
        let t = token(clock, 1.0, 10000.0);
        let (prior, _) = evaluate(t.clone(), None, vec![], &Rules::default(), clock);
        let (view, _) = evaluate(
            t.clone(),
            Some(&prior),
            vec![],
            &Rules::default(),
            clock + chrono::Duration::seconds(600),
        );
        assert_eq!(view.state, "DISCOVERED");
        let (expired, _) = evaluate(
            t,
            Some(&prior),
            vec![],
            &Rules::default(),
            clock + chrono::Duration::seconds(7201),
        );
        assert_eq!(expired.state, "EXPIRED");
    }
}

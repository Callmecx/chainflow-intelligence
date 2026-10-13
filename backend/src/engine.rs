use crate::{config::Rules, models::*, structures};
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
        category: match severity {
            "RISK" => "MARKET_RISK",
            "CONFIRMATION" => "CONFIRMATION",
            "DATA_FAULT" => "DATA_FAULT",
            _ => "OBSERVATION",
        }
        .into(),
        rule_version: structures::RULE_VERSION.into(),
        source_at: t.fetched_at.clone(),
        baseline_price_usd: t.price_usd,
        baseline_liquidity_usd: t.liquidity_usd,
        factors: json!([]),
        evidence_snapshot: serde_json::to_value(t).unwrap_or(Value::Null),
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
        && points
            .iter()
            .all(|p| p.price_usd.is_some_and(|v| v.is_finite() && v > 0.0))
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
    let mut label = previous
        .map(|p| p.label.clone())
        .unwrap_or_else(|| "NEW".into());
    let mut why = Vec::new();
    let mut evidence = Vec::new();
    let mut incomplete = vec!["HONEYPOT / TOP10 / SMART_MONEY = UNKNOWN；未独立验证".into()];
    let mut emitted = Vec::new();
    let fresh = timestamp(&t.fetched_at).is_some_and(|at| {
        at <= clock + chrono::Duration::seconds(5)
            && (clock - at).num_seconds() <= rules.max_sample_gap_seconds
    });
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
            timestamp(&p.at).is_some_and(|at| {
                at <= clock
                    && timestamp(&t.fetched_at).is_some_and(|end| at <= end)
                    && (clock - at).num_seconds() <= rules.max_sample_gap_seconds + 300
            })
        })
        .cloned()
        .collect();
    let same_source_previous = previous.is_some_and(|p| {
        !t.source.is_empty()
            && p.token.source == t.source
            && timestamp(&t.fetched_at)
                .zip(timestamp(&p.token.fetched_at))
                .is_some_and(|(now, before)| {
                    let gap = (now - before).num_seconds();
                    gap > 0 && gap <= rules.max_sample_gap_seconds
                })
    });
    let sufficient = !t.source.is_empty()
        && enough(&recent, 300, rules.max_sample_gap_seconds)
        && recent.iter().all(|p| p.source == t.source);
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
            if gap > 0 && gap <= rules.max_sample_gap_seconds && prev.token.source == t.source {
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
                if let Some(pct) = change(prev.token.price_usd, t.price_usd)
                    .filter(|pct| *pct > 0.0 && *pct >= rules.drawdown_pct)
                {
                    emitted.push(signal(
                        &t,
                        "PRICE_JUMP_UP",
                        "RISK",
                        &state,
                        "相邻真实样本价格向上跳变，需复核报价与市场波动，暂停确认升级",
                        vec![format!(
                            "price_change={pct:.2}%; interval={gap}s; threshold={}%; source={}",
                            rules.drawdown_pct, t.source
                        )],
                    ));
                }
                if prev
                    .token
                    .volume_24h_usd
                    .zip(t.volume_24h_usd)
                    .is_some_and(|(a, b)| a > 0.0 && b > a * 1.5)
                    && change(prev.token.price_usd, t.price_usd).is_some_and(|v| v < -5.0)
                {
                    emitted.push(signal(
                        &t,
                        "VOLUME_PRICE_DIVERGENCE",
                        "RISK",
                        &state,
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
        // A strict five-minute sample window: insufficient polling resolution is
        // UNKNOWN, never filled with invented intermediate prices.
        let decline: Vec<_> = history
            .iter()
            .filter(|p| {
                timestamp(&p.at)
                    .zip(timestamp(&t.fetched_at))
                    .is_some_and(|(at, end)| at <= end && (end - at).num_seconds() <= 300)
            })
            .cloned()
            .collect();
        if !t.source.is_empty()
            && enough(&decline, 0, rules.max_sample_gap_seconds)
            && decline.iter().all(|p| p.source == t.source)
            && decline.last().is_some_and(|p| {
                timestamp(&p.at) == timestamp(&t.fetched_at) && p.price_usd == t.price_usd
            })
            && decline.windows(2).all(|w| w[1].price_usd < w[0].price_usd)
        {
            if let Some(pct) =
                change(decline[0].price_usd, t.price_usd).filter(|pct| *pct <= -rules.drawdown_pct)
            {
                risk = true;
                emitted.push(signal(
                    &t,
                    "CONTINUOUS_DRAWDOWN",
                    "RISK",
                    "INVALIDATED",
                    "近5分钟同源连续采样逐次下降，累计回撤达到阈值",
                    vec![format!(
                        "sample_count={}; from={}; to={}; cumulative_change={pct:.2}%; source={}",
                        decline.len(),
                        decline[0].at,
                        t.fetched_at,
                        t.source
                    )],
                ));
            }
        }
        if let (Some(buys), Some(sells)) = (t.buys_5m, t.sells_5m) {
            let total = buys.saturating_add(sells);
            if total >= 20 && sells as f64 / total as f64 >= 0.85 {
                emitted.push(signal(
                    &t,
                    "SELL_COUNT_IMBALANCE",
                    "RISK",
                    &state,
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
    let mut structure = structures::analyze(&t, &history, rules, clock);
    // Carry a reference only across adjacent, same-source observations. Evaluate
    // its failure before confirmation, even if market-cap eligibility is lost.
    let reference = previous
        .filter(|_| fresh && same_source_previous)
        .and_then(|prev| {
            let r = &prev.structure["breakout_reference"];
            let valid = r["source"].as_str() == Some(t.source.as_str())
                && r["ceiling"]
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v > 0.0)
                && r["at"].as_str().and_then(timestamp).is_some_and(|at| {
                    timestamp(&t.fetched_at)
                        .is_some_and(|end| at < end && (end - at).num_seconds() <= 1800)
                });
            valid.then(|| r.clone())
        });
    if let Some(r) = &reference {
        structure["breakout_reference"] = r.clone();
        if r["ceiling"]
            .as_f64()
            .zip(t.price_usd)
            .is_some_and(|(ceiling, price)| {
                price.is_finite() && price > 0.0 && price < ceiling * 0.97
            })
        {
            risk = true;
            emitted.push(signal(
                &t,
                "FALSE_BREAKOUT",
                "RISK",
                "INVALIDATED",
                "突破后跌回已记录平台下方",
                vec![format!(
                    "ceiling={}; price={:?}; breakout_at={}; source={}",
                    r["ceiling"], t.price_usd, r["at"], t.source
                )],
            ));
            structure["breakout_reference"] = Value::Null;
        }
    }
    let has_market_risk = emitted.iter().any(|s| s.severity == "RISK");
    let last_risk = previous
        .and_then(|p| p.timeline.iter().rev().find(|s| s.severity == "RISK"))
        .and_then(|s| timestamp(&s.at));
    let can_recover = state == "INVALIDATED"
        && fresh
        && !has_market_risk
        && sufficient
        && eligible
        && last_risk.is_some_and(|at| (clock - at).num_seconds() >= rules.recovery_seconds);
    if risk {
        state = "INVALIDATED".into();
        label = "RISK".into();
        why.push("触发风险规则，停止确认升级".into());
    } else if age >= rules.observation_ttl_seconds {
        state = "EXPIRED".into();
        label = "EXPIRED".into();
        why.push("观察TTL已到，仍保留行情采样及结果研究".into());
    } else if can_recover {
        state = "WATCH".into();
        label = "RECOVERY_WATCH".into();
        why.push("风险冷却后恢复观察；需重新经过候选与确认，保留先前失效记录".into());
    } else if state == "INVALIDATED" {
        label = "RISK".into();
        why.push("先前风险已使本轮观察失效，不自动重新确认".into());
    } else if fresh
        && !has_market_risk
        && t.price_usd.is_some_and(|price| price > 0.0)
        && previous.is_some()
    {
        if eligible {
            let delta = recent
                .first()
                .and_then(|p| change(p.price_usd, t.price_usd));
            why.push("满足配置的新池年龄、市值、流动性观察条件".into());
            if sufficient
                && same_source_previous
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
        why.push("等待后续有效真实采样；没有可用价格时不升级状态".into());
    }
    // Independent prospective schemes. A score alone never confirms a signal.
    if fresh
        && eligible
        && !has_market_risk
        && same_source_previous
        && !can_recover
        && old_state != "INVALIDATED"
        && age >= rules.confirmation_delay_seconds
        && age < rules.observation_ttl_seconds
    {
        if structure["multi_period"] == true {
            emitted.push(signal(
                &t,
                "MULTI_PERIOD_5M_15M",
                "CONFIRMATION",
                "CONFIRMED",
                "5m/15m同源采样低点同步抬高",
                vec![format!(
                    "5m={}; 15m={}",
                    structure["windows"]["5m"], structure["windows"]["15m"]
                )],
            ));
        }
        // An existing retest must take precedence over another breakout reading;
        // otherwise a strong reclaim resets retested_at and can never confirm.
        if let Some(reference) = &reference {
            let ceiling = reference["ceiling"].as_f64().unwrap();
            if let Some(price) = t.price_usd.filter(|v| v.is_finite() && *v > 0.0) {
                if reference["retested_at"].is_null() && (0.98..=1.02).contains(&(price / ceiling))
                {
                    structure["breakout_reference"]["retested_at"] = json!(t.fetched_at);
                    emitted.push(signal(
                        &t,
                        "PLATFORM_RETEST",
                        "WATCH",
                        &state,
                        "价格回到先前突破平台附近，尚未确认站稳",
                        vec![format!(
                            "ceiling={ceiling}; price={price}; source={}",
                            t.source
                        )],
                    ));
                } else if reference["retested_at"]
                    .as_str()
                    .and_then(timestamp)
                    .is_some_and(|at| {
                        timestamp(&t.fetched_at).is_some_and(|last| {
                            last > at && (last - at).num_seconds() <= rules.max_sample_gap_seconds
                        })
                    })
                    && price > ceiling * (1.0 + rules.breakout_buffer_pct / 100.0)
                    && sufficient
                {
                    emitted.push(signal(
                        &t,
                        "RETEST_RECLAIM",
                        "CONFIRMATION",
                        "CONFIRMED",
                        "已记录回踩后，后续有效采样重新站上平台",
                        vec![format!(
                            "ceiling={ceiling}; price={price}; retested_at={}; source={}",
                            reference["retested_at"], t.source
                        )],
                    ));
                    state = "CONFIRMED".into();
                    label = "MOMENTUM_WATCH".into();
                    structure["breakout_reference"] = Value::Null;
                }
            }
        } else if structure["breakout"] == true {
            emitted.push(signal(
                &t,
                "PLATFORM_BREAKOUT",
                "WATCH",
                &state,
                "采样价格越过先前压缩平台，等待回踩或后续确认",
                vec![format!(
                    "platform_high={}; buffer_pct={}",
                    structure["platform_high"], rules.breakout_buffer_pct
                )],
            ));
            structure["breakout_reference"] = json!({"at":t.fetched_at,"source":t.source,"ceiling":structure["platform_high"],"retested_at":null});
        }
        if structure["activity_recovery"] == true {
            emitted.push(signal(
                &t,
                "ACTIVITY_REVIVAL",
                "WATCH",
                &state,
                "低活跃后连续样本买卖笔数恢复，非资金吸筹判断",
                vec![structure["windows"]["5m"].to_string()],
            ));
        }
    }
    if emitted.iter().any(|s| s.rule == "MULTI_PERIOD_5M_15M") && state != "INVALIDATED" {
        state = "CONFIRMED".into();
        label = "MOMENTUM_WATCH".into();
    }
    // Final state is resolved before emitting transitions or persisting any
    // prospective confirmation. Risks never coexist with a new confirmation.
    if emitted.iter().any(|s| s.severity == "RISK") {
        emitted.retain(|s| s.severity != "CONFIRMATION");
    }
    if state != old_state {
        emitted.push(signal(&t,&format!("STATE_{state}"),if state=="CONFIRMED" {"CONFIRMATION"} else {"WATCH"},&state,"规则观察状态变化，不构成投资建议",vec![format!("{old_state} -> {state}; observation_age={age}s; eligible={eligible}; sufficient_samples={sufficient}")]));
    }
    for event in &mut emitted {
        event.at = clock.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        event.factors = structure["factors"].clone();
        event.rule_version = format!(
            "{}-{:x}",
            structures::RULE_VERSION,
            serde_json::to_vec(rules)
                .unwrap_or_default()
                .iter()
                .fold(14695981039346656037u64, |hash, b| (hash ^ u64::from(*b))
                    .wrapping_mul(1099511628211))
        );
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
            structure,
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
            source: "fixture".into(),
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
    fn missing_price_does_not_upgrade_discovery() {
        let start = Utc::now();
        let (prior, _) = evaluate(
            token(start, 1.0, 10000.0),
            None,
            vec![],
            &Rules::default(),
            start,
        );
        let later = start + chrono::Duration::seconds(120);
        let mut t = token(later, 1.0, 10000.0);
        t.price_usd = None;
        let (view, events) = evaluate(t, Some(&prior), vec![], &Rules::default(), later);
        assert_eq!(view.state, "DISCOVERED");
        assert!(!events.iter().any(|s| s.rule.starts_with("STATE_")));
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
                source: "fixture".into(),
                ..Default::default()
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
            Some(&view),
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
            source: "fixture".into(),
            ..Default::default()
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
                source: "fixture".into(),
                ..Default::default()
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

    fn sample_history(start: DateTime<Utc>, step: i64, prices: &[f64]) -> Vec<Point> {
        prices
            .iter()
            .enumerate()
            .map(|(i, price)| Point {
                at: (start + chrono::Duration::seconds(i as i64 * step)).to_rfc3339(),
                price_usd: Some(*price),
                liquidity_usd: Some(10000.0),
                source: "fixture".into(),
                ..Default::default()
            })
            .collect()
    }
    fn prior_view(start: DateTime<Utc>, at: DateTime<Utc>, price: f64) -> TokenView {
        let (mut view, _) = evaluate(
            token(at, price, 10000.0),
            None,
            vec![],
            &Rules::default(),
            at,
        );
        view.first_seen_at = start.to_rfc3339();
        view.state = "WATCH".into();
        view
    }

    #[test]
    fn breakout_retest_and_reclaim_survive_a_second_breakout_reading() {
        let start = timestamp("2026-10-13T00:00:00Z").unwrap();
        let rules = Rules::default();
        let at = start + chrono::Duration::seconds(960);
        let prices = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.03, 1.0, 1.06];
        let prior = prior_view(start, at - chrono::Duration::seconds(120), 1.0);
        let (breakout, events) = evaluate(
            token(at, 1.03, 10000.0),
            Some(&prior),
            sample_history(start, 120, &prices[..9]),
            &rules,
            at,
        );
        assert!(events.iter().any(|s| s.rule == "PLATFORM_BREAKOUT"));
        assert_eq!(
            breakout.structure["breakout_reference"]["source"],
            "fixture"
        );
        let at = at + chrono::Duration::seconds(120);
        let (retest, events) = evaluate(
            token(at, 1.0, 10000.0),
            Some(&breakout),
            sample_history(start, 120, &prices[..10]),
            &rules,
            at,
        );
        assert!(events.iter().any(|s| s.rule == "PLATFORM_RETEST"));
        assert_eq!(
            retest.structure["breakout_reference"]["at"],
            breakout.token.fetched_at
        );
        let at = at + chrono::Duration::seconds(120);
        let (reclaim, events) = evaluate(
            token(at, 1.06, 10000.0),
            Some(&retest),
            sample_history(start, 120, &prices),
            &rules,
            at,
        );
        assert_eq!(reclaim.structure["breakout"], true);
        assert_eq!(reclaim.state, "CONFIRMED");
        assert!(events.iter().any(|s| s.rule == "RETEST_RECLAIM"));
        assert!(!events.iter().any(|s| s.rule == "PLATFORM_BREAKOUT"));
        assert!(reclaim.structure["breakout_reference"].is_null());
    }

    #[test]
    fn false_breakout_takes_priority_over_same_cycle_momentum_confirmation() {
        let start = timestamp("2026-10-13T00:00:00Z").unwrap();
        let at = start + chrono::Duration::seconds(600);
        let mut prior = prior_view(start, at - chrono::Duration::seconds(120), 0.94);
        prior.state = "CONFIRMATION_CANDIDATE".into();
        prior.structure = json!({"breakout_reference":{"source":"fixture","at":(at-chrono::Duration::seconds(240)).to_rfc3339(),"ceiling":1.0,"retested_at":null}});
        let history = sample_history(start, 120, &[0.80, 0.82, 0.84, 0.86, 0.94, 0.96]);
        let (view, events) = evaluate(
            token(at, 0.96, 10000.0),
            Some(&prior),
            history,
            &Rules::default(),
            at,
        );
        assert_eq!(view.state, "INVALIDATED");
        assert!(events.iter().any(|s| s.rule == "FALSE_BREAKOUT"));
        assert!(events.iter().any(|s| s.rule == "STATE_INVALIDATED"));
        assert!(!events
            .iter()
            .any(|s| s.severity == "CONFIRMATION" || s.rule == "STATE_CONFIRMED"));
        assert!(view.structure["breakout_reference"].is_null());
        // Loss of eligibility cannot hide failure of an already observed structure.
        let mut missing_cap = token(at, 0.96, 10000.0);
        missing_cap.market_cap_usd = None;
        let (_, events) = evaluate(missing_cap, Some(&prior), vec![], &Rules::default(), at);
        assert!(events.iter().any(|s| s.rule == "FALSE_BREAKOUT"));
    }

    #[test]
    fn source_switch_cannot_confirm_or_reuse_a_breakout_reference() {
        let start = timestamp("2026-10-13T00:00:00Z").unwrap();
        let at = start + chrono::Duration::seconds(600);
        let mut prior = prior_view(start, at - chrono::Duration::seconds(120), 1.0);
        prior.state = "CONFIRMATION_CANDIDATE".into();
        prior.structure = json!({"breakout_reference":{"source":"other","at":(at-chrono::Duration::seconds(240)).to_rfc3339(),"ceiling":1.0,"retested_at":prior.token.fetched_at}});
        let prices = [1.0, 1.01, 1.02, 1.03, 1.04, 1.08];
        let mut history = sample_history(start, 120, &prices);
        history[2].source = "other".into();
        let (view, events) = evaluate(
            token(at, 1.08, 10000.0),
            Some(&prior),
            history,
            &Rules::default(),
            at,
        );
        assert_ne!(view.state, "CONFIRMED");
        assert!(!events.iter().any(|s| s.severity == "CONFIRMATION"));
        assert!(view.structure["breakout_reference"].is_null());
        prior.token.source = "other".into();
        let (_, events) = evaluate(
            token(at, 1.08, 10000.0),
            Some(&prior),
            sample_history(start, 120, &prices),
            &Rules::default(),
            at,
        );
        assert!(!events.iter().any(|s| s.severity == "CONFIRMATION"));
    }

    #[test]
    fn upward_jump_is_risk_and_never_a_new_confirmation() {
        let start = timestamp("2026-10-13T00:00:00Z").unwrap();
        let at = start + chrono::Duration::seconds(600);
        let mut prior = prior_view(start, at - chrono::Duration::seconds(120), 1.04);
        prior.state = "CONFIRMATION_CANDIDATE".into();
        let (view, events) = evaluate(
            token(at, 1.5, 10000.0),
            Some(&prior),
            sample_history(start, 120, &[1.0, 1.01, 1.02, 1.03, 1.04, 1.5]),
            &Rules::default(),
            at,
        );
        assert_ne!(view.state, "CONFIRMED");
        assert!(events
            .iter()
            .any(|s| s.rule == "PRICE_JUMP_UP" && s.severity == "RISK"));
        assert!(!events.iter().any(|s| s.severity == "CONFIRMATION"));
        // A warning is independent of an already confirmed historical state.
        prior.state = "CONFIRMED".into();
        let (view, events) = evaluate(
            token(at, 1.5, 10000.0),
            Some(&prior),
            vec![],
            &Rules::default(),
            at,
        );
        assert_eq!(view.state, "CONFIRMED");
        assert!(events.iter().any(|s| s.rule == "PRICE_JUMP_UP"));
        assert!(!events.iter().any(|s| s.severity == "CONFIRMATION"));
    }

    #[test]
    fn continuous_drawdown_needs_four_fresh_same_source_samples() {
        let start = timestamp("2026-10-13T00:00:00Z").unwrap();
        let at = start + chrono::Duration::seconds(180);
        let prior = prior_view(start, at - chrono::Duration::seconds(60), 0.84);
        let history = sample_history(start, 60, &[1.0, 0.92, 0.84, 0.75]);
        let (view, events) = evaluate(
            token(at, 0.75, 10000.0),
            Some(&prior),
            history.clone(),
            &Rules::default(),
            at,
        );
        assert_eq!(view.state, "INVALIDATED");
        assert!(events.iter().any(|s| s.rule == "CONTINUOUS_DRAWDOWN"));
        assert!(!events.iter().any(|s| s.rule == "DRAWDOWN"));
        let mut mixed = history.clone();
        mixed[1].source = "other".into();
        let mut future = history.clone();
        future[3].at = (at + chrono::Duration::seconds(60)).to_rfc3339();
        let mut upward = history.clone();
        upward[2].price_usd = Some(0.93);
        for incomplete in [history[..3].to_vec(), mixed, future, upward] {
            let (_, events) = evaluate(
                token(at, 0.75, 10000.0),
                Some(&prior),
                incomplete,
                &Rules::default(),
                at,
            );
            assert!(!events.iter().any(|s| s.rule == "CONTINUOUS_DRAWDOWN"));
        }
    }
}

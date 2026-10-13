//! Persistent best-effort Telegram delivery through the application's existing SQLite writer.
//! Queue helpers never open a database, spawn writers, or perform network requests.
use crate::models::{canonical_address, timestamp, valid_address, Signal, TokenView};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fmt,
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug)]
pub struct NotificationPolicy {
    pub cooldown_seconds: i64,
    pub ttl_seconds: i64,
    pub max_source_age_seconds: i64,
    pub min_interval_seconds: i64,
    pub max_attempts: u32,
    pub send_watch: bool,
}
impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            cooldown_seconds: 900,
            ttl_seconds: 600,
            max_source_age_seconds: 300,
            min_interval_seconds: 5,
            max_attempts: 4,
            send_watch: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "reason")]
pub enum EnqueueResult {
    Queued,
    Duplicate,
    Suppressed,
    Disabled,
    Ignored(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimedNotification {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub attempts: u32,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status")]
pub enum DeliveryOutcome {
    Sent {
        message_id: i64,
    },
    Retry {
        code: String,
        retry_after_seconds: Option<u64>,
    },
    Uncertain {
        code: String,
    },
    Permanent {
        code: String,
    },
}

#[derive(Clone, Debug)]
pub struct DataFaultEvent {
    pub id: String,
    pub source: String,
    pub at: String,
    pub consecutive_failures: u64,
    pub last_success_at: Option<String>,
    /// A short application-owned error class, never raw transport errors or URLs with credentials.
    pub error_class: String,
}

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS notification_queue (
      id TEXT PRIMARY KEY, scope TEXT NOT NULL, kind TEXT NOT NULL, priority INTEGER NOT NULL,
      text TEXT NOT NULL, state TEXT NOT NULL, created_at INTEGER NOT NULL, source_at INTEGER NOT NULL,
      expires_at INTEGER NOT NULL, next_attempt_at INTEGER NOT NULL, attempts INTEGER NOT NULL DEFAULT 0,
      updated_at INTEGER NOT NULL, last_code TEXT, message_id INTEGER);
      CREATE INDEX IF NOT EXISTS notification_ready ON notification_queue(state,next_attempt_at,priority);
      CREATE INDEX IF NOT EXISTS notification_cooldown ON notification_queue(scope,created_at);
      CREATE TABLE IF NOT EXISTS notification_attempts (
      notification_id TEXT NOT NULL, attempt INTEGER NOT NULL, at INTEGER NOT NULL,
      outcome TEXT NOT NULL, code TEXT NOT NULL, PRIMARY KEY(notification_id,attempt));
      CREATE TABLE IF NOT EXISTS notification_control (id INTEGER PRIMARY KEY CHECK(id=1),next_send_at INTEGER NOT NULL);
      INSERT OR IGNORE INTO notification_control(id,next_send_at) VALUES(1,0);")?;
    Ok(())
}

/// Call once on startup, never while an active worker has an in-flight request.
pub fn recover(conn: &Connection, now: i64) -> Result<Value> {
    let uncertain=conn.execute("UPDATE notification_queue SET state='UNCERTAIN',last_code='PROCESS_RESTART_AFTER_CLAIM',updated_at=?1 WHERE state='SENDING'",[now])?;
    let expired = expire(conn, now)?;
    conn.execute(
        "UPDATE notification_control SET next_send_at=MAX(next_send_at,?1) WHERE id=1",
        [now.saturating_add(5)],
    )?;
    Ok(json!({"recovered_uncertain":uncertain,"expired":expired}))
}

fn expire(conn: &Connection, now: i64) -> Result<usize> {
    Ok(conn.execute("UPDATE notification_queue SET state='EXPIRED',last_code='EVENT_EXPIRED',updated_at=?1 WHERE state IN ('PENDING','RETRY') AND expires_at<=?1",[now])?)
}

pub fn enqueue_signal(
    conn: &Connection,
    signal: &Signal,
    view: &TokenView,
    now: i64,
    policy: &NotificationPolicy,
    enabled: bool,
) -> Result<EnqueueResult> {
    if !enabled {
        return Ok(EnqueueResult::Disabled);
    }
    let token = &view.token;
    if !token.valid()
        || token.pool_address.len() > 80
        || !token
            .pool_address
            .chars()
            .all(|c| c.is_ascii_alphanumeric())
        || signal.chain != token.chain
        || canonical_address(&signal.chain, &signal.token_address)
            != canonical_address(&token.chain, &token.token_address)
        || canonical_address(&signal.chain, &signal.pool_address)
            != canonical_address(&token.chain, &token.pool_address)
    {
        return Ok(EnqueueResult::Ignored("ENTITY_MISMATCH".into()));
    }
    if signal.chain == "system"
        || signal.category == "DATA_FAULT"
        || signal.rule.starts_with("SOURCE_")
        || signal.rule.starts_with("DATA_")
    {
        return Ok(EnqueueResult::Ignored("USE_DATA_FAULT_CHANNEL".into()));
    }
    let Some(at) = timestamp(&signal.at).map(|v| v.timestamp()) else {
        return Ok(EnqueueResult::Ignored("INVALID_EVENT_TIME".into()));
    };
    if !signal.source_at.is_empty() && timestamp(&signal.source_at) != timestamp(&token.fetched_at)
    {
        return Ok(EnqueueResult::Ignored("SOURCE_SNAPSHOT_MISMATCH".into()));
    }
    let Some(source_at) = timestamp(&token.fetched_at).map(|v| v.timestamp()) else {
        return Ok(EnqueueResult::Ignored("INVALID_SOURCE_TIME".into()));
    };
    if source_at > now + 30
        || at > now + 30
        || now - source_at > policy.max_source_age_seconds
        || now - at > policy.ttl_seconds
    {
        return Ok(EnqueueResult::Ignored("STALE_OR_FUTURE_EVENT".into()));
    }
    if signal.evidence.iter().all(|v| v.trim().is_empty()) {
        return Ok(EnqueueResult::Ignored("MISSING_EVIDENCE".into()));
    }
    let (kind, priority) = match signal.severity.as_str() {
        "RISK" => ("MARKET_RISK", 100),
        "CONFIRMATION"
            if signal.state == "CONFIRMED" && token.price_usd.is_some_and(|v| v > 0.0) =>
        {
            ("CONFIRMATION", 70)
        }
        "WATCH" if policy.send_watch => ("WATCH", 10),
        _ => return Ok(EnqueueResult::Ignored("NOT_FORMAL_NOTIFICATION".into())),
    };
    let scope = format!(
        "{}:{}:{}:{}:{}",
        token.chain,
        canonical_address(&token.chain, &token.token_address),
        canonical_address(&token.chain, &token.pool_address),
        signal.rule,
        kind
    );
    let candidate = Candidate {
        id: signal.id.clone(),
        scope,
        kind: kind.into(),
        priority,
        text: format_signal_card(signal, view),
        at,
        source_at,
        expires_at: at
            .saturating_add(policy.ttl_seconds.max(1))
            .min(source_at.saturating_add(policy.max_source_age_seconds.max(1))),
    };
    enqueue(conn, candidate, now, policy)
}

pub fn enqueue_data_fault(
    conn: &Connection,
    event: &DataFaultEvent,
    now: i64,
    policy: &NotificationPolicy,
    enabled: bool,
) -> Result<EnqueueResult> {
    if !enabled {
        return Ok(EnqueueResult::Disabled);
    }
    if event.consecutive_failures < 3 {
        return Ok(EnqueueResult::Ignored("FAILURE_NOT_SUSTAINED".into()));
    }
    let Some(at) = timestamp(&event.at).map(|v| v.timestamp()) else {
        return Ok(EnqueueResult::Ignored("INVALID_EVENT_TIME".into()));
    };
    if at > now + 30 || now - at > policy.ttl_seconds {
        return Ok(EnqueueResult::Ignored("STALE_OR_FUTURE_EVENT".into()));
    }
    let source = safe_line(&event.source, 80);
    let text=format!("ChainFlow｜数据故障\n来源：{source}\n连续失败：{} 次\n观察UTC：{}\n最近成功UTC：{}\n故障类别：{}\n\n数据中断与市场风险分别记录；不能据此推断代币流动性撤出。\n操作：查看本机来源状态与日志，恢复前不依据失效行情升级正式信号。",
        event.consecutive_failures,event.at,event.last_success_at.as_deref().unwrap_or("UNKNOWN"),safe_code(&event.error_class));
    enqueue(
        conn,
        Candidate {
            id: event.id.clone(),
            scope: format!("DATA_FAULT:{source}"),
            kind: "DATA_FAULT".into(),
            priority: 50,
            text,
            at,
            source_at: at,
            expires_at: at.saturating_add(policy.ttl_seconds.max(1)),
        },
        now,
        policy,
    )
}

struct Candidate {
    id: String,
    scope: String,
    kind: String,
    priority: i64,
    text: String,
    at: i64,
    source_at: i64,
    expires_at: i64,
}
fn enqueue(
    conn: &Connection,
    candidate: Candidate,
    now: i64,
    policy: &NotificationPolicy,
) -> Result<EnqueueResult> {
    if candidate.id.is_empty() || candidate.id.len() > 512 || candidate.expires_at <= now {
        return Ok(EnqueueResult::Ignored("INVALID_OR_EXPIRED_EVENT".into()));
    }
    if conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM notification_queue WHERE id=?1)",
        [&candidate.id],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(EnqueueResult::Duplicate);
    }
    let cooldown=conn.query_row("SELECT EXISTS(SELECT 1 FROM notification_queue WHERE scope=?1 AND created_at>?2 AND state IN ('PENDING','RETRY','SENDING','SENT','UNCERTAIN','FAILED'))",params![candidate.scope,now.saturating_sub(policy.cooldown_seconds.max(0))],|r|r.get::<_,bool>(0))?;
    let state = if cooldown { "SUPPRESSED" } else { "PENDING" };
    conn.execute("INSERT INTO notification_queue(id,scope,kind,priority,text,state,created_at,source_at,expires_at,next_attempt_at,updated_at,last_code) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11)",params![candidate.id,candidate.scope,candidate.kind,candidate.priority,candidate.text,state,candidate.at,candidate.source_at,candidate.expires_at,now,if cooldown {Some("RULE_COOLDOWN")}else{None}])?;
    Ok(if cooldown {
        EnqueueResult::Suppressed
    } else {
        EnqueueResult::Queued
    })
}

/// Atomic claim and shared throttle. Execute through Store::call, outside an existing transaction.
pub fn claim(
    conn: &mut Connection,
    now: i64,
    policy: &NotificationPolicy,
) -> Result<Option<ClaimedNotification>> {
    let tx = conn.transaction()?;
    expire(&tx, now)?;
    let next: i64 = tx.query_row(
        "SELECT next_send_at FROM notification_control WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    if next > now {
        tx.commit()?;
        return Ok(None);
    }
    let row=tx.query_row("SELECT id,kind,text,attempts,expires_at FROM notification_queue WHERE state IN ('PENDING','RETRY') AND next_attempt_at<=?1 AND expires_at>?1 AND attempts<?2 ORDER BY priority DESC,created_at ASC,id ASC LIMIT 1",params![now,policy.max_attempts.max(1)],|r|Ok(ClaimedNotification{id:r.get(0)?,kind:r.get(1)?,text:r.get(2)?,attempts:r.get::<_,u32>(3)?+1,expires_at:r.get(4)?})).optional()?;
    if let Some(row) = &row {
        tx.execute("UPDATE notification_queue SET state='SENDING',attempts=?2,updated_at=?3,last_code=NULL WHERE id=?1",params![row.id,row.attempts,now])?;
        tx.execute(
            "UPDATE notification_control SET next_send_at=?1 WHERE id=1",
            [now.saturating_add(policy.min_interval_seconds.max(3))],
        )?;
    }
    tx.commit()?;
    Ok(row)
}

pub fn finish(
    conn: &Connection,
    row: &ClaimedNotification,
    outcome: &DeliveryOutcome,
    now: i64,
    policy: &NotificationPolicy,
) -> Result<()> {
    let transaction = conn.unchecked_transaction()?;
    let conn = &transaction;
    let (state, code, next, message_id) = match outcome {
        DeliveryOutcome::Sent { message_id } => {
            ("SENT", "DELIVERED".into(), now, Some(*message_id))
        }
        DeliveryOutcome::Uncertain { code } => ("UNCERTAIN", safe_code(code), now, None),
        DeliveryOutcome::Permanent { code } => ("FAILED", safe_code(code), now, None),
        DeliveryOutcome::Retry {
            code,
            retry_after_seconds,
        } => {
            let backoff = 5_i64.saturating_mul(1_i64 << row.attempts.min(8)).min(300);
            let delay =
                backoff.max(retry_after_seconds.unwrap_or_default().min(i64::MAX as u64) as i64);
            let next = now.saturating_add(delay);
            if code == "RATE_LIMIT" {
                conn.execute(
                    "UPDATE notification_control SET next_send_at=MAX(next_send_at,?1) WHERE id=1",
                    [next],
                )?;
            }
            if row.attempts >= policy.max_attempts.max(1) {
                ("FAILED", "RETRIES_EXHAUSTED".into(), now, None)
            } else if next >= row.expires_at {
                ("EXPIRED", "RETRY_AFTER_EVENT_EXPIRY".into(), next, None)
            } else {
                ("RETRY", safe_code(code), next, None)
            }
        }
    };
    let changed=conn.execute("UPDATE notification_queue SET state=?2,last_code=?3,next_attempt_at=?4,updated_at=?5,message_id=?6 WHERE id=?1 AND state='SENDING' AND attempts=?7",params![row.id,state,code,next,now,message_id,row.attempts])?;
    if changed > 0 {
        conn.execute("INSERT OR IGNORE INTO notification_attempts(notification_id,attempt,at,outcome,code) VALUES(?1,?2,?3,?4,?5)",params![row.id,row.attempts,now,state,code])?;
    }
    transaction.commit()?;
    Ok(())
}

/// Safe public status: never expose destination, credentials, message bodies, or message IDs.
pub fn status(conn: &Connection, configured: bool, enabled: bool) -> Result<Value> {
    let mut counts = serde_json::Map::new();
    let mut statement =
        conn.prepare("SELECT state,count(*) FROM notification_queue GROUP BY state")?;
    for row in statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))? {
        let (state, count) = row?;
        counts.insert(state, json!(count));
    }
    let last=conn.query_row("SELECT state,last_code,updated_at FROM notification_queue WHERE state IN ('SENT','RETRY','UNCERTAIN','FAILED') ORDER BY updated_at DESC LIMIT 1",[],|r|Ok(json!({"state":r.get::<_,String>(0)?,"code":r.get::<_,Option<String>>(1)?,"at_unix":r.get::<_,i64>(2)?}))).optional()?;
    let sent = counts
        .get("SENT")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    Ok(
        json!({"configured":configured,"enabled":enabled,"verification":if sent>0 {"DELIVERED_RECORDED"}else{"PENDING"},"queue":counts,"last_delivery":last,"exactly_once":false}),
    )
}

/// Presence-only status; no configuration values are returned or logged.
pub fn environment_status() -> (bool, bool) {
    let configured = std::env::var("TELEGRAM_BOT_TOKEN").is_ok_and(|v| !v.trim().is_empty())
        && std::env::var("TELEGRAM_CHAT_ID").is_ok_and(|v| !v.trim().is_empty());
    let requested = std::env::var("TELEGRAM_ENABLED")
        .is_ok_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1"));
    (configured, configured && requested)
}

fn money(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| format!("${v:.8}"))
        .unwrap_or_else(|| "UNKNOWN".into())
}
fn safe_line(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}
fn evidence(lines: &[String], max: usize) -> String {
    if lines.is_empty() {
        "UNKNOWN".into()
    } else {
        lines
            .iter()
            .take(max)
            .map(|v| safe_line(v, 220))
            .collect::<Vec<_>>()
            .join("；")
    }
}
fn safe_code(code: &str) -> String {
    match code {
        "RATE_LIMIT"
        | "SERVER_ERROR"
        | "CONNECT_FAILED"
        | "TIMEOUT_AFTER_SEND"
        | "NETWORK_OUTCOME_UNKNOWN"
        | "INVALID_RESPONSE"
        | "INVALID_JSON"
        | "AUTH_REJECTED"
        | "DESTINATION_REJECTED"
        | "BAD_REQUEST"
        | "HTTP_REJECTED"
        | "SOURCE_ERROR"
        | "SOURCE_UNAVAILABLE"
        | "UNAVAILABLE"
        | "STALE"
        | "SERVICE_ERROR" => code.into(),
        _ => "REDACTED_ERROR".into(),
    }
}
fn retry_after(value: Option<&str>, now: SystemTime) -> Option<u64> {
    let value = value?;
    value.parse::<u64>().ok().or_else(|| {
        httpdate::parse_http_date(value).ok().map(|at| {
            let remaining = at.duration_since(now).unwrap_or_default();
            remaining
                .as_secs()
                .saturating_add(u64::from(remaining.subsec_nanos() > 0))
        })
    })
}
pub fn format_signal_card(signal: &Signal, view: &TokenView) -> String {
    let t = &view.token;
    let heading = match signal.severity.as_str() {
        "RISK" => "市场风险提醒",
        "CONFIRMATION" => "规则正式确认",
        _ => "观察事件",
    };
    let link = if valid_address(&t.chain, &t.token_address)
        && t.pool_address.chars().all(|c| c.is_ascii_alphanumeric())
        && t.pool_address.len() <= 80
    {
        if t.source == "geckoterminal" {
            format!(
                "https://www.geckoterminal.com/{}/pools/{}",
                t.chain, t.pool_address
            )
        } else {
            format!("https://dexscreener.com/{}/{}", t.chain, t.pool_address)
        }
    } else {
        "UNKNOWN".into()
    };
    let text=format!("ChainFlow｜{heading}\n{} ({}) · {}\nMint/合约：{}\n池：{}\n首次发现UTC：{}\n采集UTC：{}\n事件UTC：{}\n价格：{}\n市值 MarketCap：{}\n完全稀释估值 FDV：{}\n池流动性：{}\n池24h成交量：{}\n观察状态：{} · 规则：{}\n\n触发原因：{}\n证据：{}\n风险/缺失：{}\n失效条件：{}\n来源：{} / {}\n详情：{}\n\n仅为公开数据规则观察，不是买入建议；Honeypot / Top10 / 聪明钱：UNKNOWN。抽样无法覆盖采样间真实极值。",
        safe_line(&t.name,70),safe_line(&t.symbol,30),t.chain,t.token_address,t.pool_address,view.first_seen_at,t.fetched_at,signal.at,money(t.price_usd),money(t.market_cap_usd),money(t.fdv_usd),money(t.liquidity_usd),money(t.volume_24h_usd),safe_line(&signal.state,40),safe_line(&signal.rule,90),safe_line(&signal.message,240),evidence(&signal.evidence,3),evidence(&view.incomplete_data,2),evidence(&view.invalidation,2),safe_line(&t.source,80),safe_line(&t.dex,80),link);
    let mut used = 0usize;
    text.chars()
        .take_while(|c| {
            used += c.len_utf16();
            used <= 4000
        })
        .collect()
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, notification: &ClaimedNotification) -> DeliveryOutcome;
}

/// Secrets deliberately have no Serialize or derived Debug implementation.
pub struct TelegramTransport {
    client: Client,
    token: String,
    chat_id: String,
    base_url: String,
}
impl fmt::Debug for TelegramTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TelegramTransport { configured: true, credentials: [REDACTED] }")
    }
}
impl TelegramTransport {
    pub fn from_env() -> Result<Option<Self>> {
        if !environment_status().1 {
            return Ok(None);
        }
        Self::from_values(
            std::env::var("TELEGRAM_BOT_TOKEN").ok(),
            std::env::var("TELEGRAM_CHAT_ID").ok(),
        )
    }
    pub fn from_values(token: Option<String>, chat_id: Option<String>) -> Result<Option<Self>> {
        let token = token.unwrap_or_default().trim().to_string();
        let chat_id = chat_id.unwrap_or_default().trim().to_string();
        if token.is_empty() || chat_id.is_empty() {
            return Ok(None);
        }
        let valid_token = token.split_once(':').is_some_and(|(id, key)| {
            !id.is_empty()
                && id.bytes().all(|v| v.is_ascii_digit())
                && key.len() >= 20
                && key
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || v == b'_' || v == b'-')
        });
        let valid_chat = chat_id.parse::<i64>().is_ok()
            || (chat_id.starts_with('@')
                && chat_id.len() > 1
                && chat_id[1..]
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || v == b'_'));
        if !valid_token || !valid_chat {
            return Err(anyhow!(
                "Telegram configuration is invalid; values are redacted"
            ));
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(12))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| anyhow!("Telegram HTTP client initialization failed"))?;
        Ok(Some(Self {
            client,
            token,
            chat_id,
            base_url: "https://api.telegram.org".into(),
        }))
    }
}

#[async_trait]
impl Transport for TelegramTransport {
    async fn send(&self, notification: &ClaimedNotification) -> DeliveryOutcome {
        let url = format!("{}/bot{}/sendMessage", self.base_url, self.token);
        let response=self.client.post(url).json(&json!({"chat_id":self.chat_id,"text":notification.text,"link_preview_options":{"is_disabled":true},"allow_paid_broadcast":false})).send().await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                return if error.is_timeout() {
                    DeliveryOutcome::Uncertain {
                        code: "TIMEOUT_AFTER_SEND".into(),
                    }
                } else if error.is_connect() {
                    DeliveryOutcome::Retry {
                        code: "CONNECT_FAILED".into(),
                        retry_after_seconds: None,
                    }
                } else {
                    DeliveryOutcome::Uncertain {
                        code: "NETWORK_OUTCOME_UNKNOWN".into(),
                    }
                }
            }
        };
        let status = response.status();
        let header_delay = retry_after(
            response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok()),
            SystemTime::now(),
        );
        let body = match response.json::<Value>().await {
            Ok(body) => body,
            Err(error) => {
                return if error.is_timeout() {
                    DeliveryOutcome::Uncertain {
                        code: "TIMEOUT_AFTER_SEND".into(),
                    }
                } else if status == StatusCode::TOO_MANY_REQUESTS {
                    DeliveryOutcome::Retry {
                        code: "RATE_LIMIT".into(),
                        retry_after_seconds: header_delay,
                    }
                } else if status.is_server_error() {
                    DeliveryOutcome::Retry {
                        code: "SERVER_ERROR".into(),
                        retry_after_seconds: header_delay,
                    }
                } else if matches!(status.as_u16(), 400 | 401 | 403) {
                    DeliveryOutcome::Permanent {
                        code: match status.as_u16() {
                            401 => "AUTH_REJECTED",
                            403 => "DESTINATION_REJECTED",
                            _ => "BAD_REQUEST",
                        }
                        .into(),
                    }
                } else {
                    DeliveryOutcome::Uncertain {
                        code: "INVALID_JSON".into(),
                    }
                }
            }
        };
        // Never include response descriptions, errors or response chat objects in persisted status.
        let error_code = body["error_code"]
            .as_u64()
            .unwrap_or(status.as_u16() as u64);
        if status == StatusCode::TOO_MANY_REQUESTS || error_code == 429 {
            return DeliveryOutcome::Retry {
                code: "RATE_LIMIT".into(),
                retry_after_seconds: match (
                    body["parameters"]["retry_after"].as_u64(),
                    header_delay,
                ) {
                    (Some(body), Some(header)) => Some(body.max(header)),
                    (body, header) => body.or(header),
                },
            };
        }
        if status.is_server_error() || error_code >= 500 {
            return DeliveryOutcome::Retry {
                code: "SERVER_ERROR".into(),
                retry_after_seconds: header_delay,
            };
        }
        if status.is_success() && body["ok"].as_bool() == Some(true) {
            return match body["result"]["message_id"].as_i64() {
                Some(message_id) if message_id > 0 => DeliveryOutcome::Sent { message_id },
                _ => DeliveryOutcome::Uncertain {
                    code: "INVALID_RESPONSE".into(),
                },
            };
        }
        DeliveryOutcome::Permanent {
            code: match error_code {
                401 => "AUTH_REJECTED",
                403 => "DESTINATION_REJECTED",
                400 => "BAD_REQUEST",
                _ => "HTTP_REJECTED",
            }
            .into(),
        }
    }
}

#[cfg(test)]
mod tests;

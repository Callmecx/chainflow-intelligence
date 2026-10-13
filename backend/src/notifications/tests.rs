use super::*;
use crate::models::TokenSnapshot;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const NOW: i64 = 1_791_878_400;
#[test]
fn retry_after_date_rounds_up_and_invalid_header_uses_backoff() {
    assert_eq!(retry_after(Some("12"), SystemTime::UNIX_EPOCH), Some(12));
    assert_eq!(
        retry_after(
            Some("Thu, 01 Jan 1970 00:00:20 GMT"),
            SystemTime::UNIX_EPOCH + Duration::from_millis(500)
        ),
        Some(20)
    );
    assert_eq!(retry_after(Some("bad"), SystemTime::UNIX_EPOCH), None);
}
fn utc(at: i64) -> String {
    chrono::DateTime::from_timestamp(at, 0)
        .unwrap()
        .to_rfc3339()
}
fn fixture(id: &str) -> (Signal, TokenView) {
    let token = TokenSnapshot {
        chain: "solana".into(),
        token_address: "So11111111111111111111111111111111111111112".into(),
        pool_address: "Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE".into(),
        name: "记录测试币".into(),
        symbol: "FIXTURE".into(),
        dex: "orca".into(),
        price_usd: Some(100.0),
        liquidity_usd: Some(20_000.),
        volume_24h_usd: Some(80_000.),
        market_cap_usd: None,
        fdv_usd: Some(123_000.),
        fetched_at: utc(NOW),
        source: "dexscreener".into(),
        ..Default::default()
    };
    let signal = Signal {
        id: id.into(),
        chain: token.chain.clone(),
        token_address: token.token_address.clone(),
        pool_address: token.pool_address.clone(),
        rule: "BREAKOUT_RETEST".into(),
        severity: "CONFIRMATION".into(),
        at: utc(NOW),
        state: "CONFIRMED".into(),
        message: "回踩后连续样本重新站稳".into(),
        evidence: vec!["只使用过去真实样本：3点 > 已验证支撑".into()],
        ..Default::default()
    };
    let view = TokenView {
        token,
        label: "MOMENTUM_WATCH".into(),
        state: "CONFIRMED".into(),
        first_seen_at: utc(NOW - 600),
        why: vec![],
        evidence: vec![],
        incomplete_data: vec!["Honeypot UNKNOWN".into()],
        invalidation: vec!["跌破观察支撑或数据中断".into()],
        history: vec![],
        timeline: vec![],
        outcome: Value::Null,
        structure: Value::Null,
    };
    (signal, view)
}
fn database() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    init_schema(&conn).unwrap();
    conn
}
fn state(conn: &Connection, id: &str) -> String {
    conn.query_row(
        "SELECT state FROM notification_queue WHERE id=?1",
        [id],
        |r| r.get(0),
    )
    .unwrap()
}
fn enqueue_fixture(conn: &Connection, id: &str) -> EnqueueResult {
    let (s, v) = fixture(id);
    enqueue_signal(conn, &s, &v, NOW, &NotificationPolicy::default(), true).unwrap()
}

#[test]
fn card_has_complete_identity_unknowns_separate_valuations_and_source_link() {
    let (s, v) = fixture("one");
    let text = format_signal_card(&s, &v);
    for field in [
        "记录测试币",
        "FIXTURE",
        "solana",
        &v.token.token_address,
        &v.token.pool_address,
        "首次发现UTC",
        "采集UTC",
        "观察状态",
        "触发原因",
        "证据",
        "风险/缺失",
        "失效条件",
        "来源",
        "https://dexscreener.com/solana/",
        "市值 MarketCap：UNKNOWN",
        "完全稀释估值 FDV：$123000.00000000",
    ] {
        assert!(text.contains(field), "missing {field}");
    }
    assert!(text.encode_utf16().count() <= 4096);
}
#[test]
fn untrusted_names_do_not_enable_telegram_markup_or_break_length_budget() {
    let (s, mut v) = fixture("one");
    v.token.name = "<b>not markup</b>\n".repeat(1000);
    v.incomplete_data = vec!["🦀".repeat(2000); 10];
    let text = format_signal_card(&s, &v);
    assert!(text.encode_utf16().count() <= 4000);
    assert!(text.contains(&v.token.token_address));
    assert!(text.contains(&v.token.pool_address));
}
#[test]
fn disabled_or_incomplete_configuration_cannot_backfill_queue() {
    assert!(TelegramTransport::from_values(None, None)
        .unwrap()
        .is_none());
    assert!(TelegramTransport::from_values(Some("fixture".into()), None)
        .unwrap()
        .is_none());
    let c = database();
    let (s, v) = fixture("one");
    assert_eq!(
        enqueue_signal(&c, &s, &v, NOW, &NotificationPolicy::default(), false).unwrap(),
        EnqueueResult::Disabled
    );
    assert_eq!(status(&c, false, false).unwrap()["verification"], "PENDING");
    assert_eq!(
        c.query_row("SELECT count(*) FROM notification_queue", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn configuration_and_public_status_never_expose_secrets() {
    let token = "123456789:THIS_IS_A_FAKE_FIXTURE_TOKEN_123456";
    let chat = "-987654321";
    let transport = TelegramTransport::from_values(Some(token.into()), Some(chat.into()))
        .unwrap()
        .unwrap();
    let debug = format!("{transport:?}");
    assert!(!debug.contains(token));
    assert!(!debug.contains(chat));
    let error =
        TelegramTransport::from_values(Some("sensitive_bad_token".into()), Some(chat.into()))
            .unwrap_err()
            .to_string();
    assert!(!error.contains("sensitive_bad_token"));
    assert!(!error.contains(chat));
    let public = status(&database(), true, true).unwrap().to_string();
    assert!(!public.contains(token));
    assert!(!public.contains(chat));
    assert!(!public.contains("chat_id"));
    assert!(!public.contains("token"));
}
#[test]
fn only_fresh_confirmed_or_evidence_risk_is_enqueued() {
    let c = database();
    let (mut s, mut v) = fixture("one");
    let p = NotificationPolicy::default();
    s.state = "CONFIRMATION_CANDIDATE".into();
    assert!(matches!(
        enqueue_signal(&c, &s, &v, NOW, &p, true).unwrap(),
        EnqueueResult::Ignored(_)
    ));
    s.state = "CONFIRMED".into();
    v.token.fetched_at = utc(NOW - 301);
    assert!(matches!(
        enqueue_signal(&c, &s, &v, NOW, &p, true).unwrap(),
        EnqueueResult::Ignored(_)
    ));
    v.token.fetched_at = utc(NOW);
    s.severity = "RISK".into();
    s.evidence.clear();
    assert!(matches!(
        enqueue_signal(&c, &s, &v, NOW, &p, true).unwrap(),
        EnqueueResult::Ignored(_)
    ));
    s.evidence
        .push("liquidity 10000 -> 4000 from recorded samples".into());
    assert_eq!(
        enqueue_signal(&c, &s, &v, NOW, &p, true).unwrap(),
        EnqueueResult::Queued
    );
}
#[test]
fn watch_default_off_and_entity_mismatch_rejected() {
    let c = database();
    let (mut s, v) = fixture("one");
    s.severity = "WATCH".into();
    assert!(matches!(
        enqueue_signal(&c, &s, &v, NOW, &NotificationPolicy::default(), true).unwrap(),
        EnqueueResult::Ignored(_)
    ));
    s.severity = "RISK".into();
    s.pool_address = "other".into();
    assert_eq!(
        enqueue_signal(&c, &s, &v, NOW, &NotificationPolicy::default(), true).unwrap(),
        EnqueueResult::Ignored("ENTITY_MISMATCH".into())
    );
}
#[test]
fn dedup_cooldown_and_independent_pools() {
    let c = database();
    assert_eq!(enqueue_fixture(&c, "one"), EnqueueResult::Queued);
    assert_eq!(enqueue_fixture(&c, "one"), EnqueueResult::Duplicate);
    assert_eq!(enqueue_fixture(&c, "two"), EnqueueResult::Suppressed);
    let (mut s, mut v) = fixture("three");
    v.token.pool_address = "58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2".into();
    s.pool_address = v.token.pool_address.clone();
    assert_eq!(
        enqueue_signal(&c, &s, &v, NOW, &NotificationPolicy::default(), true).unwrap(),
        EnqueueResult::Queued
    );
    let (mut b, bv) = fixture("crosschain");
    b.chain = "bsc".into();
    assert!(matches!(
        enqueue_signal(&c, &b, &bv, NOW, &NotificationPolicy::default(), true).unwrap(),
        EnqueueResult::Ignored(_)
    ));
}
#[test]
fn risk_priority_and_persistent_global_interval() {
    let mut c = database();
    enqueue_fixture(&c, "confirmation");
    let (mut s, v) = fixture("risk");
    s.severity = "RISK".into();
    s.rule = "LIQUIDITY_DROP".into();
    enqueue_signal(&c, &s, &v, NOW, &NotificationPolicy::default(), true).unwrap();
    let p = NotificationPolicy::default();
    let row = claim(&mut c, NOW, &p).unwrap().unwrap();
    assert_eq!(row.id, "risk");
    assert!(claim(&mut c, NOW + 1, &p).unwrap().is_none());
    assert_eq!(
        claim(&mut c, NOW + 5, &p).unwrap().unwrap().id,
        "confirmation"
    );
}
#[test]
fn retries_respect_cooldown_and_429_blocks_whole_queue() {
    let mut c = database();
    enqueue_fixture(&c, "one");
    let p = NotificationPolicy::default();
    let row = claim(&mut c, NOW, &p).unwrap().unwrap();
    finish(
        &c,
        &row,
        &DeliveryOutcome::Retry {
            code: "RATE_LIMIT".into(),
            retry_after_seconds: Some(60),
        },
        NOW,
        &p,
    )
    .unwrap();
    assert_eq!(state(&c, "one"), "RETRY");
    assert!(claim(&mut c, NOW + 59, &p).unwrap().is_none());
    assert_eq!(claim(&mut c, NOW + 60, &p).unwrap().unwrap().attempts, 2);
}
#[test]
fn timeout_is_uncertain_and_never_automatically_resent() {
    let mut c = database();
    enqueue_fixture(&c, "one");
    let p = NotificationPolicy::default();
    let row = claim(&mut c, NOW, &p).unwrap().unwrap();
    finish(
        &c,
        &row,
        &DeliveryOutcome::Uncertain {
            code: "TIMEOUT_AFTER_SEND".into(),
        },
        NOW,
        &p,
    )
    .unwrap();
    assert_eq!(state(&c, "one"), "UNCERTAIN");
    assert!(claim(&mut c, NOW + 10, &p).unwrap().is_none());
    assert_eq!(status(&c, true, true).unwrap()["verification"], "PENDING");
}
#[test]
fn restart_recovers_inflight_as_uncertain_and_expires_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queue.sqlite");
    let p = NotificationPolicy::default();
    {
        let mut c = Connection::open(&path).unwrap();
        init_schema(&c).unwrap();
        enqueue_fixture(&c, "one");
        claim(&mut c, NOW, &p).unwrap().unwrap();
        let (mut s, v) = fixture("other");
        s.rule = "OTHER".into();
        enqueue_signal(&c, &s, &v, NOW, &p, true).unwrap();
    }
    let mut c = Connection::open(&path).unwrap();
    init_schema(&c).unwrap();
    let report = recover(&c, NOW + 700).unwrap();
    assert_eq!(report["recovered_uncertain"], 1);
    assert_eq!(report["expired"], 1);
    assert_eq!(state(&c, "one"), "UNCERTAIN");
    assert!(claim(&mut c, NOW + 710, &p).unwrap().is_none());
}

#[test]
fn recent_pending_survives_restart_without_bursting() {
    let mut c = database();
    enqueue_fixture(&c, "one");
    let p = NotificationPolicy::default();
    let report = recover(&c, NOW + 20).unwrap();
    assert_eq!(report["recovered_uncertain"], 0);
    assert!(claim(&mut c, NOW + 21, &p).unwrap().is_none());
    assert_eq!(claim(&mut c, NOW + 25, &p).unwrap().unwrap().id, "one");
}

#[test]
fn delivery_state_and_attempt_audit_commit_atomically() {
    let mut c = database();
    enqueue_fixture(&c, "one");
    let p = NotificationPolicy::default();
    let row = claim(&mut c, NOW, &p).unwrap().unwrap();
    c.execute_batch("CREATE TRIGGER reject_audit BEFORE INSERT ON notification_attempts BEGIN SELECT RAISE(ABORT,'fixture database write failure'); END;").unwrap();
    assert!(finish(&c, &row, &DeliveryOutcome::Sent { message_id: 1 }, NOW, &p).is_err());
    assert_eq!(state(&c, "one"), "SENDING");
    recover(&c, NOW + 1).unwrap();
    assert_eq!(state(&c, "one"), "UNCERTAIN");
}

#[test]
fn provider_snapshot_mismatch_and_expired_retry_are_not_delivered() {
    let mut c = database();
    let (mut s, v) = fixture("mismatch");
    s.source_at = utc(NOW - 5);
    assert_eq!(
        enqueue_signal(&c, &s, &v, NOW, &NotificationPolicy::default(), true).unwrap(),
        EnqueueResult::Ignored("SOURCE_SNAPSHOT_MISMATCH".into())
    );
    enqueue_fixture(&c, "one");
    let p = NotificationPolicy::default();
    let row = claim(&mut c, NOW, &p).unwrap().unwrap();
    finish(
        &c,
        &row,
        &DeliveryOutcome::Retry {
            code: "RATE_LIMIT".into(),
            retry_after_seconds: Some(3600),
        },
        NOW,
        &p,
    )
    .unwrap();
    assert_eq!(state(&c, "one"), "EXPIRED");
    assert!(claim(&mut c, NOW + 4000, &p).unwrap().is_none());
}
#[test]
fn retry_budget_permanent_failure_and_attempt_audit_are_persisted() {
    let mut c = database();
    enqueue_fixture(&c, "one");
    let p = NotificationPolicy {
        max_attempts: 1,
        ..Default::default()
    };
    let row = claim(&mut c, NOW, &p).unwrap().unwrap();
    finish(
        &c,
        &row,
        &DeliveryOutcome::Retry {
            code: "SERVER_ERROR".into(),
            retry_after_seconds: None,
        },
        NOW,
        &p,
    )
    .unwrap();
    assert_eq!(state(&c, "one"), "FAILED");
    assert_eq!(
        c.query_row("SELECT count(*) FROM notification_attempts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    finish(
        &c,
        &row,
        &DeliveryOutcome::Sent { message_id: 123 },
        NOW + 1,
        &p,
    )
    .unwrap();
    assert_eq!(state(&c, "one"), "FAILED");
}
#[test]
fn sustained_data_fault_is_not_market_risk() {
    let c = database();
    let mut e = DataFaultEvent {
        id: "failure".into(),
        source: "geckoterminal_solana".into(),
        at: utc(NOW),
        consecutive_failures: 2,
        last_success_at: None,
        error_class: "SOURCE_ERROR".into(),
    };
    let p = NotificationPolicy::default();
    assert!(matches!(
        enqueue_data_fault(&c, &e, NOW, &p, true).unwrap(),
        EnqueueResult::Ignored(_)
    ));
    e.consecutive_failures = 3;
    assert_eq!(
        enqueue_data_fault(&c, &e, NOW, &p, true).unwrap(),
        EnqueueResult::Queued
    );
    let text: String = c
        .query_row("SELECT text FROM notification_queue", [], |r| r.get(0))
        .unwrap();
    assert!(text.contains("数据故障"));
    assert!(text.contains("不能据此推断"));
    assert!(!text.contains("市场风险提醒"));
}

struct MockTransport {
    outcomes: Mutex<std::collections::VecDeque<DeliveryOutcome>>,
}
#[async_trait]
impl Transport for MockTransport {
    async fn send(&self, _: &ClaimedNotification) -> DeliveryOutcome {
        self.outcomes.lock().unwrap().pop_front().unwrap()
    }
}
#[tokio::test]
async fn mock_transport_worker_flow_recovers_retry_to_sent() {
    let mut c = database();
    enqueue_fixture(&c, "one");
    let p = NotificationPolicy::default();
    let transport = MockTransport {
        outcomes: Mutex::new(
            vec![
                DeliveryOutcome::Retry {
                    code: "SERVER_ERROR".into(),
                    retry_after_seconds: None,
                },
                DeliveryOutcome::Sent { message_id: 1 },
            ]
            .into(),
        ),
    };
    let row = claim(&mut c, NOW, &p).unwrap().unwrap();
    finish(&c, &row, &transport.send(&row).await, NOW, &p).unwrap();
    let row = claim(&mut c, NOW + 10, &p).unwrap().unwrap();
    finish(&c, &row, &transport.send(&row).await, NOW + 10, &p).unwrap();
    assert_eq!(state(&c, "one"), "SENT");
    assert_eq!(
        status(&c, true, true).unwrap()["verification"],
        "DELIVERED_RECORDED"
    );
}
async fn http_transport(
    status: &str,
    headers: &str,
    body: &str,
) -> (TelegramTransport, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let response=format!("HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 16384];
        let size = stream.read(&mut request).await.unwrap();
        stream.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8_lossy(&request[..size]).into_owned()
    });
    let mut transport = TelegramTransport::from_values(
        Some("123456789:THIS_IS_A_FAKE_FIXTURE_TOKEN_123456".into()),
        Some("-987654321".into()),
    )
    .unwrap()
    .unwrap();
    transport.client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    transport.base_url = format!("http://{address}");
    (transport, task)
}
fn message() -> ClaimedNotification {
    ClaimedNotification {
        id: "fixture".into(),
        kind: "CONFIRMATION".into(),
        text: "中文测试消息".into(),
        attempts: 1,
        expires_at: NOW + 300,
    }
}
#[tokio::test]
async fn bot_api_payload_is_free_plain_text_and_success_is_verified() {
    let (t, request) = http_transport(
        "200 OK",
        "",
        r#"{"ok":true,"result":{"message_id":42,"chat":{"id":-987654321}}}"#,
    )
    .await;
    assert_eq!(
        t.send(&message()).await,
        DeliveryOutcome::Sent { message_id: 42 }
    );
    let raw = request.await.unwrap();
    assert!(raw.contains("\"allow_paid_broadcast\":false"));
    assert!(!raw.contains("parse_mode"));
}
#[tokio::test]
async fn bot_api_429_uses_response_retry_after_without_leaking_description() {
    let(t,request)=http_transport("429 Too Many Requests","Retry-After: 30\r\n",r#"{"ok":false,"error_code":429,"description":"secret token should not appear","parameters":{"retry_after":50}}"#).await;
    let result = t.send(&message()).await;
    assert_eq!(
        result,
        DeliveryOutcome::Retry {
            code: "RATE_LIMIT".into(),
            retry_after_seconds: Some(50)
        }
    );
    assert!(!serde_json::to_string(&result).unwrap().contains("secret"));
    request.await.unwrap();
}
#[tokio::test]
async fn bot_api_403_is_permanent_and_503_retries() {
    let (t, request) = http_transport(
        "403 Forbidden",
        "",
        r#"{"ok":false,"error_code":403,"description":"private chat metadata"}"#,
    )
    .await;
    assert_eq!(
        t.send(&message()).await,
        DeliveryOutcome::Permanent {
            code: "DESTINATION_REJECTED".into()
        }
    );
    request.await.unwrap();
    let (t, request) = http_transport(
        "503 Service Unavailable",
        "Retry-After: 4\r\n",
        r#"{"ok":false,"error_code":503}"#,
    )
    .await;
    assert_eq!(
        t.send(&message()).await,
        DeliveryOutcome::Retry {
            code: "SERVER_ERROR".into(),
            retry_after_seconds: Some(4)
        }
    );
    request.await.unwrap();
}

#[tokio::test]
async fn malformed_success_is_uncertain_and_header_retry_is_not_shortened() {
    let (t, request) = http_transport("200 OK", "", r#"{"ok":true,"result":{}}"#).await;
    assert_eq!(
        t.send(&message()).await,
        DeliveryOutcome::Uncertain {
            code: "INVALID_RESPONSE".into()
        }
    );
    request.await.unwrap();
    let (t, request) = http_transport(
        "429 Too Many Requests",
        "Retry-After: 90\r\n",
        r#"{"ok":false,"error_code":429,"parameters":{"retry_after":10}}"#,
    )
    .await;
    assert_eq!(
        t.send(&message()).await,
        DeliveryOutcome::Retry {
            code: "RATE_LIMIT".into(),
            retry_after_seconds: Some(90)
        }
    );
    request.await.unwrap();
}
#[tokio::test]
async fn bot_api_timeout_is_uncertain_and_contains_no_request_url() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let mut t = TelegramTransport::from_values(
        Some("123456789:THIS_IS_A_FAKE_FIXTURE_TOKEN_123456".into()),
        Some("-987654321".into()),
    )
    .unwrap()
    .unwrap();
    t.client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(20))
        .build()
        .unwrap();
    t.base_url = format!("http://{address}");
    let result = t.send(&message()).await;
    assert_eq!(
        result,
        DeliveryOutcome::Uncertain {
            code: "TIMEOUT_AFTER_SEND".into()
        }
    );
    assert!(!serde_json::to_string(&result).unwrap().contains(&t.token));
    task.await.unwrap();
}

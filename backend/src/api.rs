use crate::{config::Rules, models::*, providers::ProviderHub, storage::Store};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, HeaderMap, Method, StatusCode},
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

pub struct AppState {
    pub store: Store,
    pub hub: ProviderHub,
    pub rules: Rules,
    pub scan_lock: Mutex<()>,
    pub started_at: String,
    pub scans: AtomicU64,
    pub accepted: AtomicU64,
    pub scan_errors: AtomicU64,
    pub api_requests: AtomicU64,
    pub last_scan_ms: AtomicU64,
    pub telegram_configured: bool,
    pub telegram_enabled: bool,
}
impl AppState {
    pub fn new(store: Store, hub: ProviderHub, rules: Rules) -> Self {
        Self {
            store,
            hub,
            rules,
            scan_lock: Mutex::new(()),
            started_at: now(),
            scans: AtomicU64::new(0),
            accepted: AtomicU64::new(0),
            scan_errors: AtomicU64::new(0),
            api_requests: AtomicU64::new(0),
            last_scan_ms: AtomicU64::new(0),
            telegram_configured: false,
            telegram_enabled: false,
        }
    }
    pub async fn scan(&self) -> anyhow::Result<usize> {
        let _guard = self.scan_lock.lock().await;
        let started = std::time::Instant::now();
        let mut watchlist = self.store.watchlist().await?;
        let existing = self.store.snapshot("LOCAL_LIVE").await?;
        let mut views = existing.items;
        let pending: Vec<String> = existing.research["records"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| {
                ["1h", "2h", "6h"]
                    .iter()
                    .any(|w| r["windows"][w]["status"] == "PENDING")
            })
            .filter_map(|r| {
                Some(format!(
                    "{}:{}",
                    r["chain"].as_str()?,
                    r["token_address"].as_str()?
                ))
            })
            .collect();
        views.sort_by_key(|v| {
            (
                if pending.contains(&format!("{}:{}", v.token.chain, v.token.token_address)) {
                    0
                } else if v.state == "CONFIRMED" || v.state == "CONFIRMATION_CANDIDATE" {
                    1
                } else if v.state == "WATCH" {
                    2
                } else {
                    3
                },
                v.token.fetched_at.clone(),
            )
        });
        // Continue observing candidates even after they disappear from discovery feeds.
        for v in views {
            let k = (v.token.chain, v.token.token_address);
            if watchlist.iter().filter(|(chain, _)| chain == &k.0).count() < 80
                && !watchlist.contains(&k)
            {
                watchlist.push(k)
            }
        }
        let result = self.hub.scan(&watchlist).await;
        self.scan_errors
            .fetch_add(result.provider_errors.len() as u64, Ordering::Relaxed);
        let count = self
            .store
            .ingest(
                result.items,
                result.sources,
                result.provider_errors,
                self.rules.clone(),
            )
            .await?;
        self.scans.fetch_add(1, Ordering::Relaxed);
        self.accepted.fetch_add(count as u64, Ordering::Relaxed);
        self.last_scan_ms
            .store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        tracing::info!(
            event = "scan_complete",
            accepted = count,
            duration_ms = started.elapsed().as_millis() as u64,
            scans = self.scans.load(Ordering::Relaxed)
        );
        Ok(count)
    }
    pub async fn snapshot(&self, mode: &str) -> anyhow::Result<Snapshot> {
        let mut s = self.store.snapshot(mode).await?;
        for view in &mut s.items {
            if timestamp(&view.first_seen_at).is_some_and(|at| {
                (chrono::Utc::now() - at).num_seconds() >= self.rules.observation_ttl_seconds
            }) {
                view.state = "EXPIRED".into();
                view.label = "EXPIRED".into();
            }
        }
        s.metrics["runtime"] = self.runtime();
        s.metrics["rules"] = serde_json::to_value(&self.rules)?;
        Ok(s)
    }
    fn runtime(&self) -> Value {
        json!({"started_at":self.started_at,"pid":std::process::id(),"scans":self.scans.load(Ordering::Relaxed),"accepted_samples":self.accepted.load(Ordering::Relaxed),"provider_errors":self.scan_errors.load(Ordering::Relaxed),"api_requests":self.api_requests.load(Ordering::Relaxed),"last_scan_duration_ms":self.last_scan_ms.load(Ordering::Relaxed),"memory_rss_bytes":memory_rss(),"memory_measurement":if cfg!(target_os="linux") {"proc_status"} else {"UNAVAILABLE"}})
    }
}
fn memory_rss() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = content.lines().find(|v| v.starts_with("VmRSS:"))?;
    line.split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|v| v * 1024)
}
type ApiResult<T> = Result<Json<T>, (StatusCode, Json<Value>)>;
fn error(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"status":"SOURCE_ERROR","error":e.to_string()})),
    )
}
fn invalid() -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":"invalid chain or full token address"})),
    )
}
pub fn router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            allowed_origins()
                .iter()
                .filter_map(|v| v.parse::<axum::http::HeaderValue>().ok())
                .collect::<Vec<_>>(),
        )
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE]);
    Router::new()
        .route("/api/health", get(health))
        .route("/api/tokens", get(tokens))
        .route("/api/tokens/{chain}/{address}", get(token))
        .route("/api/sources", get(sources))
        .route("/api/signals", get(signals))
        .route("/api/metrics", get(metrics))
        .route("/api/research", get(signal_research))
        .route("/api/watchlist", get(watchlist).post(watch))
        .route("/api/watchlist/{chain}/{address}", delete(unwatch))
        .layer(DefaultBodyLimit::max(4096))
        .layer(cors)
        .with_state(state)
}
async fn health(State(s): State<Arc<AppState>>) -> ApiResult<Value> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    let snapshot = s.snapshot("LOCAL_LIVE").await.map_err(error)?;
    let telegram = s
        .store
        .notification_status(s.telegram_configured, s.telegram_enabled)
        .await
        .map_err(error)?;
    Ok(Json(
        json!({"status":if snapshot.items.is_empty(){"INITIALIZING_OR_UNAVAILABLE"}else if !snapshot.provider_errors.is_empty() || snapshot.sources.iter().any(|v|v.status=="SOURCE_ERROR" || v.status=="UNAVAILABLE" || v.last_success_at.as_deref().and_then(timestamp).is_some_and(|at|(chrono::Utc::now()-at).num_seconds()>300)){"DEGRADED"}else{"OK"},"mode":"LOCAL_LIVE","read_only_chain":true,"started_at":s.started_at,"last_scan_at":snapshot.generated_at_utc,"database":s.store.doctor().await.map_err(error)?,"runtime":s.runtime(),"notification_counts":telegram["queue"],"telegram":telegram,"signal_counts":snapshot.metrics["signal_counts"],"source_health":snapshot.metrics["source_health"]}),
    ))
}
async fn tokens(State(s): State<Arc<AppState>>) -> ApiResult<Snapshot> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    Ok(Json(s.snapshot("LOCAL_LIVE").await.map_err(error)?))
}
async fn sources(State(s): State<Arc<AppState>>) -> ApiResult<Vec<SourceStatus>> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    Ok(Json(s.snapshot("LOCAL_LIVE").await.map_err(error)?.sources))
}
async fn signals(State(s): State<Arc<AppState>>) -> ApiResult<Vec<Signal>> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    Ok(Json(s.snapshot("LOCAL_LIVE").await.map_err(error)?.signals))
}
async fn metrics(State(s): State<Arc<AppState>>) -> ApiResult<Value> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    Ok(Json(s.snapshot("LOCAL_LIVE").await.map_err(error)?.metrics))
}
async fn signal_research(State(s): State<Arc<AppState>>) -> ApiResult<Value> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    Ok(Json(
        s.store
            .research(s.rules.research_min_samples)
            .await
            .map_err(error)?,
    ))
}
async fn token(
    State(s): State<Arc<AppState>>,
    Path((chain, address)): Path<(String, String)>,
) -> ApiResult<Vec<TokenView>> {
    if !valid_address(&chain, &address) {
        return Err(invalid());
    }
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    let address = canonical_address(&chain, &address);
    let snapshot = s.snapshot("LOCAL_LIVE").await.map_err(error)?;
    let rows: Vec<_> = snapshot
        .items
        .into_iter()
        .filter(|v| v.token.chain == chain && v.token.token_address == address)
        .collect();
    if !rows.is_empty() {
        return Ok(Json(rows));
    }
    let _guard = s.scan_lock.try_lock().map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"scan in progress; retry shortly"})),
        )
    })?;
    let result = s.hub.lookup(&chain, &address).await;
    if result.items.is_empty() && !result.provider_errors.is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"SOURCE_ERROR","errors":result.provider_errors})),
        ));
    }
    // Preserve all other source health when exact lookup only touches a subset.
    let mut sources = snapshot.sources;
    for source in result.sources {
        sources.retain(|v| v.id != source.id);
        sources.push(source);
    }
    let mut errors = result.provider_errors;
    for source in &sources {
        if source.status == "SOURCE_ERROR" || source.status == "UNAVAILABLE" {
            errors.push(format!("{}: {}", source.id, source.detail));
        }
    }
    errors.sort();
    errors.dedup();
    s.store
        .ingest(result.items, sources, errors, s.rules.clone())
        .await
        .map_err(error)?;
    Ok(Json(
        s.snapshot("LOCAL_LIVE")
            .await
            .map_err(error)?
            .items
            .into_iter()
            .filter(|v| v.token.chain == chain && v.token.token_address == address)
            .collect(),
    ))
}
#[derive(Deserialize)]
struct WatchBody {
    chain: String,
    address: String,
}
fn allowed_origins() -> Vec<String> {
    let mut origins = vec![
        "http://127.0.0.1:5173".into(),
        "http://localhost:5173".into(),
        "http://127.0.0.1:8787".into(),
    ];
    if let Ok(value) = std::env::var("CHAINFLOW_FRONTEND_ORIGIN") {
        if value
            .strip_prefix("http://127.0.0.1:")
            .and_then(|v| v.parse::<u16>().ok())
            .is_some_and(|port| port >= 1024)
        {
            origins.push(value);
        }
    }
    origins
}
fn check_origin(headers: &HeaderMap) -> Result<(), (StatusCode, Json<Value>)> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !allowed_origins()
            .iter()
            .any(|v| v == origin.to_str().unwrap_or(""))
        {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({"error":"origin not allowed"})),
            ));
        }
    }
    Ok(())
}
async fn watchlist(State(s): State<Arc<AppState>>) -> ApiResult<Value> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    Ok(Json(json!(s
        .store
        .watchlist()
        .await
        .map_err(error)?
        .into_iter()
        .map(|(chain, address)| json!({"chain":chain,"address":address}))
        .collect::<Vec<_>>())))
}
async fn watch(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<WatchBody>,
) -> ApiResult<Value> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    check_origin(&headers)?;
    if !valid_address(&body.chain, &body.address) {
        return Err(invalid());
    }
    s.store
        .watch(&body.chain, &body.address, false)
        .await
        .map_err(error)?;
    Ok(Json(json!({"status":"WATCHING","next_scan":"scheduled"})))
}
async fn unwatch(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((chain, address)): Path<(String, String)>,
) -> ApiResult<Value> {
    s.api_requests.fetch_add(1, Ordering::Relaxed);
    check_origin(&headers)?;
    if !valid_address(&chain, &address) {
        return Err(invalid());
    }
    s.store.watch(&chain, &address, true).await.map_err(error)?;
    Ok(Json(json!({"status":"REMOVED"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    #[tokio::test]
    async fn health_validation_and_write_origin() {
        let d = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(
            Store::open(d.path().join("api.db").to_str().unwrap()).unwrap(),
            ProviderHub::new().unwrap(),
            Rules::default(),
        ));
        let app = router(state);
        for (url, status) in [
            ("/api/health", 200),
            ("/api/tokens/wrong/not-an-address", 400),
            ("/api/tokens", 200),
            ("/api/research", 200),
        ] {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(url)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
        }
        let response=app.oneshot(axum::http::Request::builder().uri("/api/watchlist").method("POST").header("content-type","application/json").header("origin","https://attacker.example").body(axum::body::Body::from(r#"{"chain":"solana","address":"So11111111111111111111111111111111111111112"}"#)).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

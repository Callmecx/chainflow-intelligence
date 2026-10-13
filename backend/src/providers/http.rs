use crate::models::{now, SourceStatus};
use anyhow::{anyhow, Result};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{sync::Mutex, time::Instant};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum Bucket {
    Discovery,
    Market,
    Gecko,
    Rpc,
}
impl Bucket {
    fn interval(self) -> Duration {
        Duration::from_millis(match self {
            Self::Discovery => 7500,
            Self::Market => 2000,
            Self::Gecko => 10000,
            Self::Rpc => 3000,
        })
    }
}

/// A capacity-one token bucket: a single shared permit refills at a conservative rate.
/// Cooldown survives individual requests and scan cycles, including Retry-After failures.
#[derive(Default)]
struct Limiter {
    next: HashMap<Bucket, Instant>,
}
impl Limiter {
    fn reserve(&mut self, bucket: Bucket, at: Instant) -> Result<Duration> {
        let next = self.next.get(&bucket).copied().unwrap_or(at).max(at);
        let wait = next.duration_since(at);
        if wait > Duration::from_secs(35) {
            return Err(anyhow!(
                "rate-limit cooldown; next permit in {}s",
                wait.as_secs()
            ));
        }
        self.next.insert(bucket, next + bucket.interval());
        Ok(wait)
    }
    fn defer(&mut self, bucket: Bucket, delay: Duration) {
        let until = Instant::now() + delay;
        self.next
            .entry(bucket)
            .and_modify(|v| *v = (*v).max(until))
            .or_insert(until);
    }
}

#[derive(Clone, Debug)]
pub(super) struct Response {
    pub body: Value,
    pub fetched_at: String,
}
#[derive(Clone)]
struct Cached {
    response: Response,
    expires: Instant,
}

pub(super) struct Http {
    client: Client,
    // Serialize issuance per bucket so a Retry-After cannot race an already reserved permit.
    gates: [Mutex<()>; 4],
    limiter: Mutex<Limiter>,
    cache: Mutex<HashMap<String, Cached>>,
    health: Mutex<HashMap<String, SourceStatus>>,
}
impl Http {
    pub fn new() -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .user_agent("ChainFlow-Intelligence/1.1 (read-only public research)")
                .build()?,
            gates: std::array::from_fn(|_| Mutex::new(())),
            limiter: Mutex::new(Limiter::default()),
            cache: Mutex::new(HashMap::new()),
            health: Mutex::new(HashMap::new()),
        }))
    }
    async fn record(&self, id: &str, ok: bool, elapsed: Duration, detail: String, request: bool) {
        let mut health = self.health.lock().await;
        let status = health
            .entry(id.to_string())
            .or_insert_with(|| SourceStatus {
                id: id.into(),
                ..Default::default()
            });
        status.status = if ok { "OK" } else { "SOURCE_ERROR" }.into();
        status.checked_at = now();
        if ok {
            status.last_success_at = Some(status.checked_at.clone());
        } else {
            status.errors += 1;
        }
        status.latency_ms = Some(elapsed.as_millis() as u64);
        status.requests += u64::from(request);
        status.detail = detail;
    }
    pub async fn statuses(&self) -> Vec<SourceStatus> {
        let mut values: Vec<_> = self.health.lock().await.values().cloned().collect();
        values.sort_by(|a, b| a.id.cmp(&b.id));
        values
    }
    pub async fn schema_error(&self, id: &str, detail: String) {
        self.record(id, false, Duration::ZERO, detail, false).await;
    }
    pub async fn no_coverage(&self, id: &str, detail: String) {
        if let Some(status) = self.health.lock().await.get_mut(id) {
            status.status = "UNAVAILABLE".into();
            status.detail = detail;
        }
    }
    pub async fn get(&self, id: &str, url: &str, bucket: Bucket, ttl: u64) -> Result<Response> {
        self.request(id, url, bucket, ttl, None).await
    }
    pub async fn post(&self, id: &str, url: &str, body: Value) -> Result<Response> {
        self.request(id, url, Bucket::Rpc, 60, Some(body)).await
    }
    async fn request(
        &self,
        id: &str,
        url: &str,
        bucket: Bucket,
        ttl: u64,
        body: Option<Value>,
    ) -> Result<Response> {
        let key = format!(
            "{url}|{}",
            body.as_ref().map(Value::to_string).unwrap_or_default()
        );
        if let Some(cached) = self
            .cache
            .lock()
            .await
            .get(&key)
            .filter(|v| v.expires > Instant::now())
            .cloned()
        {
            return Ok(cached.response);
        }
        for attempt in 0..3 {
            let _gate = self.gates[bucket as usize].lock().await;
            let start = Instant::now();
            let permit = self.limiter.lock().await.reserve(bucket, start);
            let wait = match permit {
                Ok(wait) => wait,
                Err(error) => {
                    self.record(id, false, start.elapsed(), error.to_string(), false)
                        .await;
                    return Err(error);
                }
            };
            tokio::time::sleep(wait).await;
            let builder = if let Some(body) = &body {
                self.client.post(url).json(body)
            } else {
                self.client.get(url)
            };
            let response = builder
                .header("Accept", "application/json;version=20230203")
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if !status.is_success() {
                        let retry =
                            status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                        let delay = retry_after(
                            response
                                .headers()
                                .get("Retry-After")
                                .and_then(|v| v.to_str().ok()),
                            SystemTime::now(),
                        )
                        .unwrap_or(Duration::from_secs(1 << attempt));
                        let detail = format!(
                            "HTTP {status}; {}",
                            if retry {
                                format!("retry cooldown {}s", delay.as_secs())
                            } else {
                                "no retry".into()
                            }
                        );
                        self.record(id, false, start.elapsed(), detail.clone(), true)
                            .await;
                        if retry {
                            self.limiter.lock().await.defer(bucket, delay);
                        }
                        if retry && attempt < 2 && delay <= Duration::from_secs(20) {
                            continue;
                        }
                        return Err(anyhow!(detail));
                    }
                    let body: Value = match response.json().await {
                        Ok(body) => body,
                        Err(error) => {
                            let detail = format!("invalid JSON: {error}");
                            self.record(id, false, start.elapsed(), detail.clone(), true)
                                .await;
                            return Err(anyhow!(detail));
                        }
                    };
                    self.record(
                        id,
                        true,
                        start.elapsed(),
                        "HTTP 200; public read-only data; local cache retains original fetched_at"
                            .into(),
                        true,
                    )
                    .await;
                    let result = Response {
                        body,
                        fetched_at: now(),
                    };
                    let mut cache = self.cache.lock().await;
                    cache.retain(|_, v| v.expires > Instant::now());
                    // Bound address-query cache growth even under repeated local searches.
                    if cache.len() >= 256 {
                        cache.clear();
                    }
                    cache.insert(
                        key,
                        Cached {
                            response: result.clone(),
                            expires: Instant::now() + Duration::from_secs(ttl),
                        },
                    );
                    return Ok(result);
                }
                Err(error) => {
                    let detail = if error.is_timeout() {
                        "request timeout".to_string()
                    } else {
                        format!("network error: {error}")
                    };
                    self.record(id, false, start.elapsed(), detail.clone(), true)
                        .await;
                    if attempt == 2 {
                        return Err(anyhow!(detail));
                    }
                    self.limiter
                        .lock()
                        .await
                        .defer(bucket, Duration::from_secs(1 << attempt));
                }
            }
        }
        unreachable!()
    }
}

pub(super) fn retry_after(value: Option<&str>, now: SystemTime) -> Option<Duration> {
    let value = value?;
    value
        .parse::<u64>()
        .map(Duration::from_secs)
        .ok()
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()
                .map(|at| at.duration_since(now).unwrap_or_default())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn mock_http(timeout: Duration) -> Arc<Http> {
        Arc::new(Http {
            // Offline fixtures must not depend on a developer's proxy configuration.
            client: Client::builder()
                .no_proxy()
                .timeout(timeout)
                .build()
                .unwrap(),
            gates: std::array::from_fn(|_| Mutex::new(())),
            limiter: Mutex::new(Limiter::default()),
            cache: Mutex::new(HashMap::new()),
            health: Mutex::new(HashMap::new()),
        })
    }
    #[test]
    fn retry_after_seconds_dates_and_invalid() {
        assert_eq!(
            retry_after(Some("12"), SystemTime::UNIX_EPOCH),
            Some(Duration::from_secs(12))
        );
        assert_eq!(
            retry_after(
                Some("Thu, 01 Jan 1970 00:00:20 GMT"),
                SystemTime::UNIX_EPOCH
            ),
            Some(Duration::from_secs(20))
        );
        assert_eq!(retry_after(Some("bad"), SystemTime::UNIX_EPOCH), None);
    }
    #[test]
    fn shared_limiter_carries_state_across_requests_and_buckets() {
        let mut limiter = Limiter::default();
        let at = Instant::now();
        assert_eq!(limiter.reserve(Bucket::Gecko, at).unwrap(), Duration::ZERO);
        assert_eq!(
            limiter.reserve(Bucket::Gecko, at).unwrap(),
            Duration::from_secs(10)
        );
        assert_eq!(limiter.reserve(Bucket::Market, at).unwrap(), Duration::ZERO);
        limiter.defer(Bucket::Gecko, Duration::from_secs(120));
        assert!(limiter.reserve(Bucket::Gecko, at).is_err());
    }
    async fn server(responses: Vec<String>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                let _ = stream.read(&mut buf).await;
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        format!("http://{address}")
    }
    #[tokio::test]
    async fn forbidden_is_not_retried() {
        let url = server(vec![
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        ])
        .await;
        let http = mock_http(Duration::from_secs(2));
        assert!(http
            .get("test", &url, Bucket::Market, 10)
            .await
            .unwrap_err()
            .to_string()
            .contains("403"));
        let status = &http.statuses().await[0];
        assert_eq!(status.requests, 1);
        assert_eq!(status.status, "SOURCE_ERROR");
    }
    #[tokio::test]
    async fn long_retry_after_is_honoured_without_blocking_scan() {
        let url=server(vec!["HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()]).await;
        let http = mock_http(Duration::from_secs(2));
        assert!(http.get("test", &url, Bucket::Market, 10).await.is_err());
        assert!(http
            .get("test", &url, Bucket::Market, 10)
            .await
            .unwrap_err()
            .to_string()
            .contains("cooldown"));
        assert_eq!(http.statuses().await[0].requests, 1);
    }
    #[tokio::test]
    async fn cache_retains_timestamp_and_request_count() {
        let url = server(vec![
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".into(),
        ])
        .await;
        let http = mock_http(Duration::from_secs(2));
        let first = http.get("test", &url, Bucket::Market, 10).await.unwrap();
        let second = http.get("test", &url, Bucket::Market, 10).await.unwrap();
        assert_eq!(first.fetched_at, second.fetched_at);
        assert_eq!(http.statuses().await[0].requests, 1);
    }
    #[tokio::test]
    async fn service_unavailable_retries_and_recovers() {
        let url=server(vec!["HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(), "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".into()]).await;
        let http = mock_http(Duration::from_secs(2));
        assert!(http.get("test", &url, Bucket::Market, 10).await.is_ok());
        let status = &http.statuses().await[0];
        assert_eq!(status.requests, 2);
        assert_eq!(status.errors, 1);
        assert_eq!(status.status, "OK");
    }
    #[tokio::test]
    async fn timeout_is_counted_without_inventing_a_response() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut streams = vec![];
            for _ in 0..3 {
                streams.push(listener.accept().await.unwrap().0);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let http = mock_http(Duration::from_millis(50));
        let error = http
            .get("timeout", &format!("http://{address}"), Bucket::Market, 10)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timeout"));
        assert_eq!(http.statuses().await[0].requests, 3);
        assert_eq!(http.statuses().await[0].errors, 3);
        assert!(http.statuses().await[0].last_success_at.is_none());
    }
}

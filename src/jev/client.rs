use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64};

#[derive(Default, Debug)]
pub struct Stats {
    pub requests: AtomicU32,
    pub cache_hits: AtomicU32,
    pub input_tokens: AtomicU64,
    pub model: Mutex<Option<String>>,
    /// `x-typesafe-request-id` of the last response seen, success or failure (surfaced in `meta`).
    pub request_id: Mutex<Option<String>>,
}

use super::cache::{DiskCache, key as cache_key};
use super::{Questions, Response};
use crate::config::Config;
use crate::exit::HunchError;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::Semaphore;

pub struct Client {
    http: reqwest::Client,
    base: String,
    key: String,
    model: String,
    sem: Arc<Semaphore>,
    cache: Option<DiskCache>,
    stats: Arc<Stats>,
}

impl Client {
    pub fn new(cfg: &Config) -> Result<Self, HunchError> {
        let key = cfg.api_key()?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!("hunch/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| HunchError::Unavailable(e.to_string()))?;
        Ok(Self {
            http,
            base: cfg.base_url.clone(),
            key,
            model: cfg.model.clone(),
            sem: Arc::new(Semaphore::new(cfg.concurrency)),
            cache: cfg.cache_dir.clone().and_then(|d| DiskCache::new(d).ok()),
            stats: cfg.stats.clone(),
        })
    }

    /// Opens the TLS connection while local work runs; the pooled connection is reused by `ask`.
    /// Only `run` calls this: it reads its tool inventory (~1 s of CPU) before its first request,
    /// so the handshake overlaps real work. On the current-thread runtime the spawned task only
    /// progresses while the caller is parked in an `.await`, so the local work must go through
    /// `spawn_blocking` (see `run::load_tools`), or prewarm races `ask` and buys nothing.
    pub fn prewarm(&self) {
        let req = self
            .http
            .get(format!("{}/v1/models", self.base))
            .bearer_auth(&self.key);
        tokio::spawn(async move {
            // Read the body too: hyper returns an HTTP/1.1 connection to the pool only once the
            // response is consumed, and the point of prewarm is that `ask` reuses it.
            if let Ok(r) = req.send().await {
                let _ = r.bytes().await;
            }
        });
    }

    pub async fn ask(
        &self,
        state: &serde_json::Value,
        questions: &Questions,
    ) -> Result<Response, HunchError> {
        let body =
            serde_json::json!({ "model": self.model, "state": state, "questions": questions });
        let bytes = serde_json::to_vec(&body).map_err(|e| HunchError::Protocol(e.to_string()))?;
        let k = cache_key(&bytes);
        if let Some(hit) = self.cache.as_ref().and_then(|c| c.get(&k)) {
            self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
            *self.stats.model.lock().unwrap() = Some(hit.model.clone());
            return Ok(hit);
        }
        let _permit = self.sem.acquire().await.expect("semaphore open");
        let url = format!("{}/v1/systemone", self.base);
        let mut last = String::new();
        let mut wait = None;
        for attempt in 0..4u32 {
            if attempt > 0 {
                // The server's `retry-after(-ms)` replaces the backoff; it never adds to it.
                let backoff = Duration::from_millis(250 * 2u64.pow(attempt - 1));
                tokio::time::sleep(wait.take().unwrap_or(backoff)).await;
            }
            let res = self
                .http
                .post(&url)
                .bearer_auth(&self.key)
                .header("content-type", "application/json")
                .body(bytes.clone())
                .send()
                .await;
            let r = match res {
                Ok(r) => r,
                Err(e) => {
                    last = e.to_string();
                    if e.is_timeout() || e.is_connect() {
                        continue;
                    }
                    return Err(HunchError::Unavailable(last));
                }
            };
            // Stored now, and again by the two arms that await an error body before returning:
            // under `try_join_all` a sibling window's response can land during that await and
            // would otherwise replace the failing request's id.
            let rid = r
                .headers()
                .get("x-typesafe-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            if rid.is_some() {
                *self.stats.request_id.lock().unwrap() = rid.clone();
            }
            let status = r.status().as_u16();
            match status {
                200 => {
                    let resp: Response = r
                        .json()
                        .await
                        .map_err(|e| HunchError::Protocol(e.to_string()))?;
                    self.stats.requests.fetch_add(1, Ordering::Relaxed);
                    self.stats
                        .input_tokens
                        .fetch_add(resp.usage.input_tokens, Ordering::Relaxed);
                    *self.stats.model.lock().unwrap() = Some(resp.model.clone());
                    if let Some(c) = &self.cache {
                        c.put(&k, &resp);
                    }
                    return Ok(resp);
                }
                401 | 403 => return Err(HunchError::BadKey(status)),
                // 413/422 = the request body was rejected: in practice state over the token budget,
                // occasionally a malformed request (a hunch bug). An input problem (exit 6), not an
                // outage (exit 4); the error kind and hint keep the two readings apart.
                413 | 422 => {
                    let text = r.text().await.unwrap_or_default();
                    if rid.is_some() {
                        *self.stats.request_id.lock().unwrap() = rid;
                    }
                    return Err(HunchError::RejectedRequest(
                        status,
                        rejection_message(&text),
                    ));
                }
                // 408 request timeout, 429 rate limit, 5xx (incl. 529 overloaded): retry after the
                // server's wait if it named one, else with backoff.
                408 | 429 | 500..=599 => {
                    wait = retry_after(r.headers());
                    last = format!("HTTP {status}");
                }
                _ => {
                    let text = r.text().await.unwrap_or_default();
                    if rid.is_some() {
                        *self.stats.request_id.lock().unwrap() = rid;
                    }
                    return Err(HunchError::Protocol(format!(
                        "HTTP {status}: {}",
                        text.chars().take(300).collect::<String>()
                    )));
                }
            }
        }
        Err(HunchError::Unavailable(last))
    }
}

/// The wait the server asked for: `retry-after-ms` (milliseconds) beats `retry-after` (whole
/// seconds; an HTTP-date is ignored), both capped at 10 s. `None` means use the backoff.
fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let num = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
    };
    num("retry-after-ms")
        .map(Duration::from_millis)
        .or_else(|| num("retry-after").map(Duration::from_secs))
        .map(|d| d.min(Duration::from_secs(10)))
}

/// The readable part of a rejected request's body. The API answers 422 in FastAPI's shape,
/// `{"detail":[{"loc":["body","state"],"msg":"..."}]}`, which becomes `state: ...`; any other
/// body is passed through. Clipped to 300 chars.
fn rejection_message(text: &str) -> String {
    let flattened = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            let parts: Vec<String> = v
                .get("detail")?
                .as_array()?
                .iter()
                .filter_map(|e| {
                    let msg = e.get("msg")?.as_str()?;
                    let path: Vec<String> = e
                        .get("loc")
                        .and_then(|l| l.as_array())
                        .map(|l| {
                            l.iter()
                                .filter_map(|x| match x {
                                    serde_json::Value::String(s) if s != "body" => Some(s.clone()),
                                    serde_json::Value::Number(n) => Some(n.to_string()),
                                    _ => None,
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(if path.is_empty() {
                        msg.to_string()
                    } else {
                        format!("{}: {msg}", path.join("."))
                    })
                })
                .collect();
            (!parts.is_empty()).then(|| parts.join("; "))
        });
    flattened
        .unwrap_or_else(|| text.to_string())
        .chars()
        .take(300)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    #[test]
    fn retry_after_prefers_the_ms_header_and_caps_at_ten_seconds() {
        let mut h = HeaderMap::new();
        assert_eq!(retry_after(&h), None);
        h.insert("retry-after", HeaderValue::from_static("2"));
        assert_eq!(retry_after(&h), Some(Duration::from_secs(2)));
        h.insert("retry-after-ms", HeaderValue::from_static("200"));
        assert_eq!(retry_after(&h), Some(Duration::from_millis(200)));
        h.insert("retry-after-ms", HeaderValue::from_static("60000"));
        assert_eq!(retry_after(&h), Some(Duration::from_secs(10)));
        // An HTTP-date (or garbage) falls back to the backoff.
        let mut d = HeaderMap::new();
        d.insert(
            "retry-after",
            HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        assert_eq!(retry_after(&d), None);
    }

    #[test]
    fn rejection_message_flattens_fastapi_detail_and_passes_other_bodies_through() {
        let fastapi = r#"{"detail":[{"loc":["body","state"],"msg":"too many tokens","type":"value_error"},{"loc":["body","questions","q1","criteria",0],"msg":"empty","type":"value_error"}]}"#;
        assert_eq!(
            rejection_message(fastapi),
            "state: too many tokens; questions.q1.criteria.0: empty"
        );
        assert_eq!(
            rejection_message(r#"{"detail":[{"msg":"bad request"}]}"#),
            "bad request"
        );
        assert_eq!(rejection_message("state too large"), "state too large");
        assert_eq!(rejection_message(r#"{"error":"x"}"#), r#"{"error":"x"}"#);
        assert_eq!(rejection_message(""), "");
        assert_eq!(rejection_message(&"x".repeat(500)).len(), 300);
    }
}

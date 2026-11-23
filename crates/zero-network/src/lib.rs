#![forbid(unsafe_code)]

pub mod credentials;
pub mod diagnostics;
pub mod rate_limit;

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::{Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::Semaphore;

pub use credentials::{CredentialError, CredentialStore, MemoryCredentialStore, OsCredentialStore};
pub use diagnostics::{ConnectionState, DiagnosticsSnapshot, NetworkDiagnostics};
pub use rate_limit::{BackoffPolicy, RateLimiter};

const MAX_CONCURRENT_REQUESTS: usize = 8;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("API error ({status}): {body}")]
    HttpStatus { status: u16, body: String },
    #[error("Rate limited after max retries on route {route}")]
    RateLimitExhausted { route: String },
    #[error("JSON decode error: {0}")]
    Decode(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    rate_limiter: Arc<RateLimiter>,
    backoff: BackoffPolicy,
    concurrency_limit: Arc<Semaphore>,
    diagnostics: Arc<NetworkDiagnostics>,
}

impl HttpClient {
    pub fn new(user_agent: &str) -> Result<Self, NetworkError> {
        let mut default_headers = HeaderMap::new();
        if let Ok(ua) = HeaderValue::from_str(user_agent) {
            default_headers.insert(USER_AGENT, ua);
        }

        let client = reqwest::Client::builder()
            .default_headers(default_headers)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .pool_max_idle_per_host(4)
            .build()?;

        Ok(Self {
            client,
            rate_limiter: Arc::new(RateLimiter::new()),
            backoff: BackoffPolicy::default(),
            concurrency_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
            diagnostics: Arc::new(NetworkDiagnostics::default()),
        })
    }

    pub fn diagnostics(&self) -> DiagnosticsSnapshot {
        self.diagnostics.snapshot()
    }

    pub fn set_state(&self, state: ConnectionState) {
        self.diagnostics.set_state(state);
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        route_key: &str,
        url: &str,
        auth_header: Option<&str>,
    ) -> Result<T, NetworkError> {
        let resp = self
            .execute_with_retry(Method::GET, route_key, url, auth_header, None::<&()>)
            .await?;
        let bytes = resp.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        route_key: &str,
        url: &str,
        auth_header: Option<&str>,
        body: &B,
    ) -> Result<T, NetworkError> {
        let resp = self
            .execute_with_retry(Method::POST, route_key, url, auth_header, Some(body))
            .await?;
        let bytes = resp.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn execute_with_retry<B: Serialize>(
        &self,
        method: Method,
        route_key: &str,
        url: &str,
        auth_header: Option<&str>,
        body: Option<&B>,
    ) -> Result<Response, NetworkError> {
        let _permit = self
            .concurrency_limit
            .acquire()
            .await
            .expect("semaphore not closed");

        for attempt in 0..=self.backoff.max_retries {
            self.rate_limiter.wait_for_slot(route_key).await;
            self.diagnostics.set_state(ConnectionState::Connecting);
            let started = Instant::now();

            let mut req = self.client.request(method.clone(), url);
            if let Some(auth) = auth_header {
                req = req.header(AUTHORIZATION, auth);
            }
            if let Some(payload) = body {
                req = req.json(payload);
            }

            match req.send().await {
                Ok(resp) => {
                    self.rate_limiter
                        .update_from_headers(route_key, resp.headers())
                        .await;
                    let status = resp.status();

                    if status == StatusCode::TOO_MANY_REQUESTS {
                        self.diagnostics.record_rate_limit();
                        let retry_after = resp
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(1.0);
                        let global = resp
                            .headers()
                            .get("x-ratelimit-global")
                            .and_then(|v| v.to_str().ok())
                            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
                        self.rate_limiter
                            .record_429(route_key, retry_after, global)
                            .await;
                        if attempt < self.backoff.max_retries {
                            self.diagnostics.record_retry();
                            continue;
                        }
                        return Err(NetworkError::RateLimitExhausted {
                            route: route_key.to_string(),
                        });
                    }

                    if status.is_server_error() && attempt < self.backoff.max_retries {
                        self.diagnostics.record_retry();
                        tokio::time::sleep(self.backoff.delay_for_attempt(attempt)).await;
                        continue;
                    }

                    if !status.is_success() {
                        let err_body = resp.text().await.unwrap_or_default();
                        let msg = format!("HTTP {}: {}", status.as_u16(), err_body);
                        self.diagnostics.record_failure(&msg);
                        return Err(NetworkError::HttpStatus {
                            status: status.as_u16(),
                            body: err_body,
                        });
                    }

                    let elapsed_ms = started.elapsed().as_millis() as u64;
                    self.diagnostics.record_success(elapsed_ms);
                    return Ok(resp);
                }
                Err(e) => {
                    if attempt < self.backoff.max_retries && (e.is_timeout() || e.is_connect()) {
                        self.diagnostics.record_retry();
                        tokio::time::sleep(self.backoff.delay_for_attempt(attempt)).await;
                        continue;
                    }
                    self.diagnostics.record_failure(e.to_string());
                    return Err(NetworkError::Transport(e));
                }
            }
        }

        Err(NetworkError::RateLimitExhausted {
            route: route_key.to_string(),
        })
    }
}

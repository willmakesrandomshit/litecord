use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub base: Duration,
    pub max: Duration,
    pub max_retries: u32,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(400),
            max: Duration::from_secs(30),
            max_retries: 4,
        }
    }
}

impl BackoffPolicy {
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let shift = attempt.min(16);
        let mult = 1u32.checked_shl(shift).unwrap_or(u32::MAX);
        self.base.saturating_mul(mult).min(self.max)
    }
}

#[derive(Debug, Clone)]
struct BucketState {
    remaining: u32,
    reset_at: Instant,
}

/// Rate-limit-aware scheduler supporting per-route buckets and global rate limits.
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, BucketState>>,
    global_reset_at: Mutex<Option<Instant>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            global_reset_at: Mutex::new(None),
        }
    }

    /// Waits if either the global limit or the route's bucket is exhausted.
    pub async fn wait_for_slot(&self, route: &str) {
        let global_wait = {
            let mut g = self.global_reset_at.lock().await;
            if let Some(until) = *g {
                let now = Instant::now();
                if until > now {
                    Some(until - now)
                } else {
                    *g = None;
                    None
                }
            } else {
                None
            }
        };
        if let Some(d) = global_wait {
            tokio::time::sleep(d).await;
        }

        let bucket_wait = {
            let buckets = self.buckets.lock().await;
            buckets.get(route).and_then(|b| {
                let now = Instant::now();
                if b.remaining == 0 && b.reset_at > now {
                    Some(b.reset_at - now)
                } else {
                    None
                }
            })
        };
        if let Some(d) = bucket_wait {
            tokio::time::sleep(d).await;
        }
    }

    /// Records rate-limit headers from an HTTP response.
    pub async fn update_from_headers(&self, route: &str, headers: &reqwest::header::HeaderMap) {
        let is_global = headers
            .get("x-ratelimit-global")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));

        let retry_after_secs = headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<f64>().ok());

        let reset_after_secs = headers
            .get("x-ratelimit-reset-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<f64>().ok())
            .or(retry_after_secs);

        let remaining = headers
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u32>().ok());

        if is_global {
            if let Some(secs) = reset_after_secs {
                let mut g = self.global_reset_at.lock().await;
                *g = Some(Instant::now() + Duration::from_secs_f64(secs.max(0.0)));
            }
            return;
        }

        if let (Some(rem), Some(secs)) = (remaining, reset_after_secs) {
            let mut buckets = self.buckets.lock().await;
            // Bound bucket map size to prevent unbounded growth across many dynamic routes.
            if buckets.len() >= 512 && !buckets.contains_key(route) {
                buckets.clear();
            }
            buckets.insert(
                route.to_string(),
                BucketState {
                    remaining: rem,
                    reset_at: Instant::now() + Duration::from_secs_f64(secs.max(0.0)),
                },
            );
        }
    }

    pub async fn record_429(&self, route: &str, retry_after_secs: f64, global: bool) {
        let until = Instant::now() + Duration::from_secs_f64(retry_after_secs.max(0.1));
        if global {
            let mut g = self.global_reset_at.lock().await;
            *g = Some(until);
        } else {
            let mut buckets = self.buckets.lock().await;
            buckets.insert(
                route.to_string(),
                BucketState {
                    remaining: 0,
                    reset_at: until,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff_caps_at_max() {
        let policy = BackoffPolicy {
            base: Duration::from_millis(100),
            max: Duration::from_millis(1000),
            max_retries: 5,
        };
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(policy.delay_for_attempt(4), Duration::from_millis(1000));
    }

    #[tokio::test]
    async fn rate_limiter_updates_and_waits_on_exhausted_bucket() {
        let rl = RateLimiter::new();
        rl.record_429("GET /test", 0.05, false).await;
        let start = Instant::now();
        rl.wait_for_slot("GET /test").await;
        assert!(start.elapsed() >= Duration::from_millis(40));
    }
}

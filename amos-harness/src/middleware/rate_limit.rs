//! Per-IP rate limiting middleware using a token bucket algorithm.
//!
//! Each unique client IP gets a bucket that refills at a configured rate.
//! When the bucket is empty, requests receive 429 Too Many Requests.
//! Stale buckets are cleaned up periodically to prevent memory growth.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use std::{
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

/// Token bucket state for a single IP address.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Shared rate limiter state. Clone-friendly (wraps Arc internally via DashMap).
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<DashMap<IpAddr, Bucket>>,
    /// Maximum tokens (burst capacity)
    capacity: f64,
    /// Tokens added per second
    refill_rate: f64,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// - `capacity`: max burst size (e.g., 30 requests)
    /// - `per_second`: sustained request rate (e.g., 10 req/s)
    pub fn new(capacity: u32, per_second: f64) -> Self {
        let limiter = Self {
            buckets: Arc::new(DashMap::new()),
            capacity: capacity as f64,
            refill_rate: per_second,
        };

        // Spawn background cleanup every 5 minutes to evict stale buckets
        let buckets = limiter.buckets.clone();
        let cap = limiter.capacity;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                let now = Instant::now();
                // Remove buckets that have been full (idle) for over 10 minutes
                buckets.retain(|_ip, bucket| {
                    let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
                    let would_be = bucket.tokens + elapsed * cap; // approximate
                    // If the bucket would be full and hasn't been touched in 10min, evict
                    !(would_be >= cap && elapsed > 600.0)
                });
            }
        });

        limiter
    }

    /// Try to consume one token for the given IP. Returns true if allowed.
    fn try_acquire(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut entry = self.buckets.entry(ip).or_insert_with(|| Bucket {
            tokens: self.capacity,
            last_refill: now,
        });

        let bucket = entry.value_mut();
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_rate).min(self.capacity);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Extract client IP from request headers (X-Forwarded-For, X-Real-IP)
/// or fall back to the connected peer address.
fn extract_client_ip(req: &Request) -> IpAddr {
    // Try X-Forwarded-For first (leftmost = original client)
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = forwarded.to_str() {
            if let Some(first) = val.split(',').next() {
                if let Ok(ip) = first.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(val) = real_ip.to_str() {
            if let Ok(ip) = val.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }

    // Fall back to peer address from connection info, or loopback
    req.extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
}

/// Axum middleware function for rate limiting.
///
/// Usage:
/// ```ignore
/// let limiter = RateLimiter::new(60, 10.0); // 60 burst, 10/s sustained
/// router.layer(axum::middleware::from_fn(move |req, next| {
///     rate_limit_middleware(req, next, limiter.clone())
/// }))
/// ```
pub async fn rate_limit_middleware(
    request: Request,
    next: Next,
    limiter: RateLimiter,
) -> Response {
    let ip = extract_client_ip(&request);

    if limiter.try_acquire(ip) {
        next.run(request).await
    } else {
        tracing::warn!(client_ip = %ip, "Rate limit exceeded");
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "1")],
            axum::Json(serde_json::json!({
                "error": "Too many requests",
                "code": "rate_limited",
                "retry_after_secs": 1
            })),
        )
            .into_response()
    }
}

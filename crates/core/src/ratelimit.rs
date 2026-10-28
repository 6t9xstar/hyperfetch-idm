use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Token bucket shared by every connection of one scope (per download and/or
/// globally). A rate of 0 disables throttling.
pub struct RateLimiter {
    rate: AtomicU64,
    bucket: Mutex<Bucket>,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(rate: u64) -> Self {
        Self {
            rate: AtomicU64::new(rate),
            bucket: Mutex::new(Bucket {
                tokens: rate as f64,
                last: Instant::now(),
            }),
        }
    }

    pub fn unlimited() -> Self {
        Self::new(0)
    }

    pub fn rate(&self) -> u64 {
        self.rate.load(Ordering::Relaxed)
    }

    pub fn set_rate(&self, rate: u64) {
        self.rate.store(rate, Ordering::Relaxed);
    }

    /// Wait until `bytes` may be transferred. Sleeps instead of blocking.
    pub async fn acquire(&self, bytes: u64) {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 || bytes == 0 {
            return;
        }

        loop {
            let current_rate = self.rate.load(Ordering::Relaxed);
            if current_rate == 0 {
                return;
            }

            let wait = {
                let mut bucket = self.bucket.lock().await;
                let now = Instant::now();
                let elapsed = now.duration_since(bucket.last).as_secs_f64();
                bucket.last = now;

                // Allow a one-second burst.
                let capacity = current_rate as f64;
                bucket.tokens = (bucket.tokens + elapsed * current_rate as f64).min(capacity);

                if bucket.tokens >= bytes as f64 {
                    bucket.tokens -= bytes as f64;
                    return;
                }

                let needed = bytes as f64 - bucket.tokens;
                Duration::from_secs_f64(needed / current_rate as f64)
            };

            tokio::time::sleep(wait.max(Duration::from_millis(1))).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn unlimited_returns_immediately() {
        let limiter = RateLimiter::unlimited();
        let start = Instant::now();
        limiter.acquire(10 << 20).await;
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn throttles_above_rate() {
        let limiter = RateLimiter::new(64 * 1024);
        let start = Instant::now();
        limiter.acquire(64 * 1024).await; // consumes the initial burst
        limiter.acquire(32 * 1024).await; // requires ~0.5 s
        assert!(start.elapsed() >= Duration::from_millis(300));
    }
}

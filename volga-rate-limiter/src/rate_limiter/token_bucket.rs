//! Tools and data structures for a token-bucket rate limiter.

use super::{RateLimiter, SystemTimeSource, TimeSource};
use dashmap::DashMap;
use std::{
    sync::{Arc, atomic::{AtomicU64, Ordering::*}},
    time::Duration
};

const DEFAULT_SCALE: u64 = 1_000_000;

/// Internal per-key state for the token bucket algorithm.
///
/// Each entry tracks:
/// - `available_tokens`: scaled number of available tokens (fixed-point),
/// - `last_refill_secs`: last time the bucket was refilled.
#[derive(Debug)]
struct Entry {
    /// Current token balance in fixed-point representation.
    available_tokens: AtomicU64,

    /// Last refill time in seconds since UNIX_EPOCH.
    last_refill_secs: AtomicU64,
}

/// A token-bucket rate limiter.
///
/// The token bucket algorithm allows bursts up to the configured capacity
/// while enforcing an average rate over time. Tokens accumulate at a steady
/// rate and are consumed per request.
///
/// ## Characteristics
///
/// - **Allows short bursts** up to the bucket capacity.
/// - **Enforces average rate** using a refill cadence.
/// - **Lock-free hot path** with atomic counters.
/// - **Lazy eviction** of inactive keys.
///
/// ## Algorithm
///
/// For each `key`:
///
/// 1. Refill tokens based on time since the last refill:
///    `tokens += elapsed * refill_rate`.
/// 2. Clamp tokens to `capacity`.
/// 3. If at least one token is available, consume it and allow the request.
/// 4. Otherwise, deny the request.
///
/// ## Eviction
///
/// Inactive entries are removed lazily when they have not been touched for
/// longer than `eviction_grace_secs`. No background jobs are required.
///
/// ## When to use
///
/// This limiter is suitable when:
///
/// - bursts should be allowed but controlled,
/// - a steady average rate is required,
/// - per-key state must stay compact and lock-free.
#[derive(Debug)]
pub struct TokenBucketRateLimiter<T: TimeSource = SystemTimeSource> {
    /// Per-key rate limiting state.
    storage: Arc<DashMap<u64, Entry>>,

    /// Maximum number of tokens in the bucket.
    capacity: u64,

    /// Precalculated: tokens added per second (fixed-point).
    refill_rate_scaled: u64,

    /// Fixed-point scaling factor for fractional refill rates.
    scale: u64,
    
    /// Precalculated scaled capacity scale * capacity (fixed-point).
    capacity_scaled: u64,

    /// Time after which inactive entries are eligible for eviction.
    eviction_grace_secs: u64,

    /// Time source used to determine the current time.
    time_source: T,
}

impl<T: TimeSource> RateLimiter for TokenBucketRateLimiter<T> {
    /// Checks whether the rate limit has been exceeded for the given `key`.
    ///
    /// Returns `true` if the request is allowed, or `false` if the rate
    /// limit has been reached.
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now = self.time_source.now_secs();

        if let Some(entry) = self.storage.get(&key) {
            let last_refill = entry.last_refill_secs.load(Acquire);
            if now.saturating_sub(last_refill) > self.eviction_grace_secs {
                drop(entry);
                self.storage.remove(&key);
            }
        }

        let entry = self.storage.entry(key).or_insert_with(|| Entry {
            available_tokens: AtomicU64::new(self.capacity_scaled),
            last_refill_secs: AtomicU64::new(now),
        });

        self.refill(entry.value(), now);

        self.try_consume(entry.value())
    }
}

impl TokenBucketRateLimiter {
    /// Creates a new token bucket rate limiter using the system clock.
    ///
    /// # Parameters
    ///
    /// - `capacity`: maximum number of tokens in the bucket.
    /// - `refill_rate`: tokens added per second.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// - `capacity * scale` overflows `u64` when computing the internal fixed-point capacity.
    /// - `refill_rate` is not finite (`NaN` or ±∞).
    /// - `refill_rate` is negative.
    /// - `refill_rate * scale` exceeds `u64::MAX` when computing the internal fixed-point refill rate.
    #[inline]
    pub fn new(capacity: u64, refill_rate: f64) -> Self {
        Self::with_time_source(capacity, refill_rate, SystemTimeSource)
    }
}

impl<T: TimeSource> TokenBucketRateLimiter<T> {
    /// Creates a [`TokenBucketRateLimiter`] with a custom [`TimeSource`].
    ///
    /// This is primarily useful for testing and deterministic scenarios.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// - `capacity * scale` overflows `u64` when computing the internal fixed-point capacity.
    /// - `refill_rate` is not finite (`NaN` or ±∞).
    /// - `refill_rate` is negative.
    /// - `refill_rate * scale` exceeds `u64::MAX` when computing the internal fixed-point refill rate.
    #[inline]
    pub fn with_time_source(capacity: u64, refill_rate: f64, time_source: T) -> Self {
        let scale: u64 = DEFAULT_SCALE;

        assert!(refill_rate.is_finite(), "refill_rate must be finite");
        assert!(refill_rate >= 0.0, "refill_rate must be >= 0");

        let scaled_f = refill_rate * scale as f64;
        assert!(scaled_f <= u64::MAX as f64, "refill_rate too large");

        let refill_rate_scaled = scaled_f.floor() as u64;

        let capacity_scaled = capacity
            .checked_mul(scale)
            .expect("capacity * scale overflow");
        
        Self {
            storage: Arc::new(DashMap::new()),
            capacity,
            refill_rate_scaled,
            scale,
            capacity_scaled,
            eviction_grace_secs: 60,
            time_source,
        }
    }

    /// Sets the eviction grace period for inactive entries.
    ///
    /// Entries that have not been accessed for longer than this duration
    /// may be removed during subsequent `check` calls.
    #[inline]
    pub fn set_eviction(&mut self, eviction: Duration) {
        self.eviction_grace_secs = eviction.as_secs();
    }

    /// Bucket capacity (max tokens).
    #[inline(always)]
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Tokens added per second.
    #[inline(always)]
    pub fn refill_rate(&self) -> f64 {
        self.refill_rate_scaled as f64 / self.scale as f64
    }

    /// Time after which inactive entries are eligible for eviction.
    #[inline(always)]
    pub fn eviction_grace_secs(&self) -> u64 {
        self.eviction_grace_secs
    }

    fn refill(&self, entry: &Entry, now: u64) {
        let mut last_refill = entry.last_refill_secs.load(Acquire);

        loop {
            if now <= last_refill {
                return;
            }

            match entry
                .last_refill_secs
                .compare_exchange(last_refill, now, AcqRel, Acquire)
            {
                Ok(_) => break,
                Err(next) => last_refill = next,
            }
        }

        let elapsed = now - last_refill;
        let add = elapsed.saturating_mul(self.refill_rate_scaled);

        let mut current = entry.available_tokens.load(Relaxed);
        loop {
            let updated = current.saturating_add(add).min(self.capacity_scaled);
            match entry
                .available_tokens
                .compare_exchange(current, updated, AcqRel, Relaxed)
            {
                Ok(_) => break,
                Err(next) => current = next,
            }
        }
    }

    fn try_consume(&self, entry: &Entry) -> bool {
        let mut current = entry.available_tokens.load(Relaxed);
        loop {
            if current < self.scale {
                return false;
            }
            let updated = current - self.scale;
            match entry
                .available_tokens
                .compare_exchange(current, updated, AcqRel, Relaxed)
            {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::MockTimeSource;

    #[test]
    fn token_bucket_allows_burst_up_to_capacity() {
        let limiter = TokenBucketRateLimiter::new(3, 1.0);
        let key = 99;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));
    }

    #[test]
    fn token_bucket_refills_over_time() {
        let time = MockTimeSource::new(100);
        let limiter = TokenBucketRateLimiter::with_time_source(2, 1.0, time.clone());
        let key = 7;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));

        time.advance(1);
        assert!(limiter.check(key));
        assert!(!limiter.check(key));

        time.advance(1);
        assert!(limiter.check(key));
    }

    #[test]
    fn token_bucket_isolated_per_key() {
        let limiter = TokenBucketRateLimiter::new(1, 1.0);

        assert!(limiter.check(1));
        assert!(!limiter.check(1));
        assert!(limiter.check(2));
    }
}

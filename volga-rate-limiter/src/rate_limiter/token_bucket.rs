//! Tools and data structures for a token-bucket rate limiter.

use super::{
    MICROS_PER_SEC, RateLimiter, SystemTimeSource, TimeSource,
    store::{TokenBucketParams, TokenBucketStore},
};
use dashmap::DashMap;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::*},
    },
    time::Duration,
};

/// Internal per-key state for the token bucket algorithm.
///
/// Each entry tracks:
/// - `available_tokens`: scaled number of available tokens (fixed-point),
/// - `last_refill_us`: last time the bucket was refilled.
#[derive(Debug)]
struct Entry {
    /// Current token balance in fixed-point representation.
    available_tokens: AtomicU64,

    /// Last refill time in microseconds (monotonic).
    last_refill_us: AtomicU64,

    /// Last access time in microseconds (for eviction).
    last_seen_us: AtomicU64,
}

/// In-memory [`TokenBucketStore`] backed by a concurrent hash map.
///
/// This is the default store used by [`TokenBucketRateLimiter`].
/// It holds per-key token state in a `DashMap` and performs lazy eviction.
#[derive(Debug, Clone)]
pub struct InMemoryTokenBucketStore {
    storage: Arc<DashMap<u64, Entry>>,
}

impl InMemoryTokenBucketStore {
    /// Creates a new empty in-memory token-bucket store.
    pub fn new() -> Self {
        Self {
            storage: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryTokenBucketStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenBucketStore for InMemoryTokenBucketStore {
    #[inline]
    fn try_consume(&self, params: TokenBucketParams) -> bool {
        let TokenBucketParams {
            key,
            now_us,
            capacity_scaled,
            refill_rate_scaled_per_sec,
            scale,
            eviction_grace_us,
        } = params;

        // Lazy eviction based on last_seen, not last_refill.
        if let Some(entry) = self.storage.get(&key) {
            let last_seen = entry.last_seen_us.load(Acquire);
            if now_us.saturating_sub(last_seen) > eviction_grace_us {
                drop(entry);
                self.storage.remove(&key);
            }
        }

        let entry = self.storage.entry(key).or_insert_with(|| Entry {
            available_tokens: AtomicU64::new(capacity_scaled),
            last_refill_us: AtomicU64::new(now_us),
            last_seen_us: AtomicU64::new(now_us),
        });

        // Touch last_seen (best-effort).
        entry.last_seen_us.store(now_us, Release);

        Self::refill(
            entry.value(),
            now_us,
            refill_rate_scaled_per_sec,
            capacity_scaled,
        );
        Self::consume(entry.value(), scale)
    }
}

impl InMemoryTokenBucketStore {
    fn refill(entry: &Entry, now_us: u64, refill_rate_scaled_per_sec: u64, capacity_scaled: u64) {
        if refill_rate_scaled_per_sec == 0 {
            return;
        }

        // Claim the time interval [last_refill, now] using CAS to avoid double-refill.
        let mut last = entry.last_refill_us.load(Acquire);
        loop {
            if now_us <= last {
                return;
            }

            match entry
                .last_refill_us
                .compare_exchange(last, now_us, AcqRel, Acquire)
            {
                Ok(_) => break,
                Err(next) => last = next,
            }
        }

        let elapsed_us = now_us - last;
        // add_scaled = elapsed_us * (tokens/sec * scale) / 1_000_000
        let num = (elapsed_us as u128) * (refill_rate_scaled_per_sec as u128);
        let add_u128 = num / (MICROS_PER_SEC as u128);
        let add = u64::try_from(add_u128).unwrap_or(u64::MAX);

        if add == 0 {
            return;
        }

        let mut current = entry.available_tokens.load(Relaxed);
        loop {
            let updated = current.saturating_add(add).min(capacity_scaled);
            match entry
                .available_tokens
                .compare_exchange(current, updated, AcqRel, Relaxed)
            {
                Ok(_) => return,
                Err(next) => current = next,
            }
        }
    }

    fn consume(entry: &Entry, scale: u64) -> bool {
        let mut current = entry.available_tokens.load(Relaxed);
        loop {
            if current < scale {
                return false;
            }
            let updated = current - scale;
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
pub struct TokenBucketRateLimiter<
    T: TimeSource = SystemTimeSource,
    S: TokenBucketStore = InMemoryTokenBucketStore,
> {
    store: S,
    capacity: u64,
    refill_rate_scaled_per_sec: u64,
    scale: u64,
    capacity_scaled: u64,
    eviction_grace_us: u64,
    time_source: T,
}

impl<T: TimeSource, S: TokenBucketStore> RateLimiter for TokenBucketRateLimiter<T, S> {
    /// Checks whether the rate limit has been exceeded for the given `key`.
    ///
    /// Returns `true` if the request is allowed, or `false` if the rate
    /// limit has been reached.
    #[inline]
    fn check(&self, key: u64) -> bool {
        self.store.try_consume(TokenBucketParams {
            key,
            now_us: self.time_source.now_micros(),
            capacity_scaled: self.capacity_scaled,
            refill_rate_scaled_per_sec: self.refill_rate_scaled_per_sec,
            scale: self.scale,
            eviction_grace_us: self.eviction_grace_us,
        })
    }
}

const DEFAULT_SCALE: u64 = MICROS_PER_SEC;
const DEFAULT_EVICTION: u64 = 60 * MICROS_PER_SEC; // 1 minute

impl TokenBucketRateLimiter {
    /// Creates a new token bucket rate limiter using the system clock
    /// and the default in-memory store.
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
    ///
    /// A `refill_rate` of `0.0` is **valid** — it means a one-time burst up to
    /// `capacity` with no subsequent refill.
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
    /// See [`TokenBucketRateLimiter::new`] for the full list of panic conditions.
    #[inline]
    pub fn with_time_source(capacity: u64, refill_rate: f64, time_source: T) -> Self {
        Self::with_time_source_and_store(
            capacity,
            refill_rate,
            time_source,
            InMemoryTokenBucketStore::new(),
        )
    }
}

impl<S: TokenBucketStore> TokenBucketRateLimiter<SystemTimeSource, S> {
    /// Creates a [`TokenBucketRateLimiter`] with a custom [`TokenBucketStore`].
    ///
    /// # Panics
    ///
    /// See [`TokenBucketRateLimiter::new`] for the full list of panic conditions.
    #[inline]
    pub fn with_store(capacity: u64, refill_rate: f64, store: S) -> Self {
        Self::with_time_source_and_store(capacity, refill_rate, SystemTimeSource, store)
    }
}

impl<T: TimeSource, S: TokenBucketStore> TokenBucketRateLimiter<T, S> {
    /// Creates a [`TokenBucketRateLimiter`] with a custom [`TimeSource`] and [`TokenBucketStore`].
    ///
    /// # Panics
    ///
    /// See [`TokenBucketRateLimiter::new`] for the full list of panic conditions.
    #[inline]
    pub fn with_time_source_and_store(
        capacity: u64,
        refill_rate: f64,
        time_source: T,
        store: S,
    ) -> Self {
        let scale: u64 = DEFAULT_SCALE;

        assert!(refill_rate.is_finite(), "refill_rate must be finite");
        assert!(refill_rate >= 0.0, "refill_rate must be >= 0");

        let scaled_f = refill_rate * scale as f64;
        assert!(scaled_f <= u64::MAX as f64, "refill_rate too large");

        let refill_rate_scaled_per_sec = scaled_f.round() as u64;

        let capacity_scaled = capacity
            .checked_mul(scale)
            .expect("capacity * scale overflow");

        Self {
            store,
            capacity,
            refill_rate_scaled_per_sec,
            scale,
            capacity_scaled,
            eviction_grace_us: DEFAULT_EVICTION,
            time_source,
        }
    }

    /// Sets the eviction grace period for inactive entries.
    ///
    /// Entries that have not been accessed for longer than this duration
    /// may be removed during subsequent `check` calls.
    #[inline]
    pub fn set_eviction(&mut self, eviction: Duration) {
        self.eviction_grace_us = eviction.as_micros().try_into().unwrap_or(u64::MAX);
    }

    /// Bucket capacity (max tokens).
    #[inline(always)]
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Tokens added per second.
    #[inline(always)]
    pub fn refill_rate(&self) -> f64 {
        self.refill_rate_scaled_per_sec as f64 / self.scale as f64
    }

    /// Time after which inactive entries are eligible for eviction.
    #[inline(always)]
    pub fn eviction_grace_secs(&self) -> u64 {
        self.eviction_grace_us / MICROS_PER_SEC
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::MockTimeSource;
    use super::*;

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

    #[test]
    fn token_bucket_with_custom_store_delegates_to_store() {
        use crate::rate_limiter::store::{TokenBucketParams, TokenBucketStore};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

        struct CountingStore {
            inner: InMemoryTokenBucketStore,
            calls: Arc<AtomicU32>,
        }
        impl TokenBucketStore for CountingStore {
            fn try_consume(&self, params: TokenBucketParams) -> bool {
                self.calls.fetch_add(1, Relaxed);
                self.inner.try_consume(params)
            }
        }

        let calls = Arc::new(AtomicU32::new(0));
        let store = CountingStore {
            inner: InMemoryTokenBucketStore::new(),
            calls: calls.clone(),
        };
        let limiter = TokenBucketRateLimiter::with_store(3, 1.0, store);

        assert!(limiter.check(99));
        assert_eq!(calls.load(Relaxed), 1);
    }

    #[test]
    fn token_bucket_zero_refill_rate_is_valid() {
        // refill_rate=0.0 is explicitly permitted — it means one burst up to capacity,
        // with no ongoing refill. This is pre-existing behaviour: the constructor allows
        // any finite non-negative rate, and the refill loop short-circuits when
        // refill_rate_scaled_per_sec == 0. This test confirms the behaviour is
        // preserved through the new with_time_source_and_store delegation path and
        // documents the semantics explicitly.
        let limiter = TokenBucketRateLimiter::new(2, 0.0);
        assert!(limiter.check(1));
        assert!(limiter.check(1));
        assert!(!limiter.check(1)); // exhausted, no refill
    }

    #[test]
    fn token_bucket_tiny_refill_rate_rounds_to_zero_scaled() {
        // 1e-10 * 1_000_000 = 0.0001, rounds to 0 — treated same as zero refill.
        // Pre-existing behaviour; test confirms the delegation path preserves it.
        let limiter = TokenBucketRateLimiter::new(1, 1e-10);
        assert!(limiter.check(1));
        assert!(!limiter.check(1));
    }

    #[test]
    #[should_panic(expected = "capacity * scale overflow")]
    fn panics_when_capacity_scaled_overflows() {
        // scale = 1_000_000
        // overflow if capacity > u64::MAX / scale
        let scale = 1_000_000u64;
        let capacity = (u64::MAX / scale) + 1;

        let _ = TokenBucketRateLimiter::with_time_source_and_store(
            capacity,
            1.0,
            SystemTimeSource,
            InMemoryTokenBucketStore::new(),
        );
    }

    #[test]
    #[should_panic(expected = "refill_rate must be finite")]
    fn panics_when_refill_rate_is_nan() {
        let _ = TokenBucketRateLimiter::with_time_source_and_store(
            1,
            f64::NAN,
            SystemTimeSource,
            InMemoryTokenBucketStore::new(),
        );
    }

    #[test]
    #[should_panic(expected = "refill_rate must be finite")]
    fn panics_when_refill_rate_is_infinite() {
        let _ = TokenBucketRateLimiter::with_time_source_and_store(
            1,
            f64::INFINITY,
            SystemTimeSource,
            InMemoryTokenBucketStore::new(),
        );
    }

    #[test]
    #[should_panic(expected = "refill_rate must be >= 0")]
    fn panics_when_refill_rate_is_negative() {
        let _ = TokenBucketRateLimiter::with_time_source_and_store(
            1,
            -0.1,
            SystemTimeSource,
            InMemoryTokenBucketStore::new(),
        );
    }

    #[test]
    #[should_panic(expected = "refill_rate too large")]
    fn panics_when_refill_rate_scaled_exceeds_u64_max() {
        // scale = 1_000_000, so anything > u64::MAX / 1e6 will overflow after scaling.
        // Using a very large value avoids edge cases with f64 rounding.
        let _ = TokenBucketRateLimiter::with_time_source_and_store(
            1,
            1e30,
            SystemTimeSource,
            InMemoryTokenBucketStore::new(),
        );
    }
}

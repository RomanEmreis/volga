//! Tools and data structures for a GCRA (Generic Cell Rate Algorithm) limiter.

use super::{RateLimiter, SystemTimeSource, TimeSource, MICROS_PER_SEC};
use dashmap::DashMap;
use std::{
    sync::{Arc, atomic::{AtomicU64, Ordering::*}},
    time::Duration
};

const DEFAULT_EVICTION: u64 = 60 * MICROS_PER_SEC; // 1 minute

/// Internal per-key state for the GCRA algorithm.
///
/// - `tat_us`: theoretical arrival time (TAT) in microseconds
/// - `last_seen_us`: last access time in microseconds (for eviction)
#[derive(Debug)]
struct Entry {
    /// Theoretical arrival time in seconds since UNIX_EPOCH.
    tat_us: AtomicU64,
    
    /// Last access time in microseconds (for eviction)
    last_seen_us: AtomicU64,
}

/// A GCRA (Generic Cell Rate Algorithm) rate limiter.
///
/// GCRA enforces an average rate with optional burst tolerance, using
/// the "theoretical arrival time" of the next allowed request.
///
/// ## Characteristics
///
/// - **Smooth traffic** with strong average rate guarantees.
/// - **Burst tolerance** controlled via a configurable burst size.
/// - **Lock-free hot path** using atomic updates.
/// - **Lazy eviction** of inactive keys.
///
/// ## Algorithm
///
/// The algorithm uses:
///
/// - `t = now`
/// - `tau = 1 / rate` (emission interval)
/// - `burst = burst_size` (maximum burst tokens)
/// - `limit = tau * burst`
///
/// Request is allowed if:
///
/// ```text
/// t + limit >= tat
/// ```
///
/// When allowed, the new `tat` is:
///
/// ```text
/// tat = max(t, tat) + tau
/// ```
///
/// ## Eviction
///
/// Entries are removed lazily when they have not been touched for longer
/// than `eviction_grace_us`. No background jobs are required.
///
/// ## When to use
///
/// This limiter is suitable when:
///
/// - smooth request pacing is desired,
/// - burst tolerance should be explicit,
/// - an O(1) per-request algorithm is needed.
#[derive(Debug)]
pub struct GcraRateLimiter<T: TimeSource = SystemTimeSource> {
    /// Per-key rate limiting state.
    storage: Arc<DashMap<u64, Entry>>,

    /// Emission interval in fixed-point microseconds.
    emission_interval_us: u64,

    /// Burst allowance in fixed-point microseconds.
    burst_allowance_us: u64,

    /// Configured burst size.
    burst: u32,

    /// Time after which inactive entries are eligible for eviction.
    eviction_grace_us: u64,

    /// Time source used to determine the current time.
    time_source: T,
}

impl<T: TimeSource> RateLimiter for GcraRateLimiter<T> {
    /// Checks whether the rate limit has been exceeded for the given `key`.
    ///
    /// Returns `true` if the request is allowed, or `false` if the rate
    /// limit has been reached.
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now_us = self.time_source.now_micros();

        // Lazy eviction based on last_seen, not TAT.
        if let Some(entry) = self.storage.get(&key) {
            let last_seen = entry.last_seen_us.load(Acquire);
            if now_us.saturating_sub(last_seen) > self.eviction_grace_us {
                drop(entry);
                self.storage.remove(&key);
            }
        }

        let entry = self.storage.entry(key).or_insert_with(|| Entry {
            tat_us: AtomicU64::new(now_us),
            last_seen_us: AtomicU64::new(now_us),
        });

        // Touch last_seen (best-effort; eviction is approximate anyway).
        entry.last_seen_us.store(now_us, Release);

        let mut current_tat = entry.tat_us.load(Relaxed);
        loop {
            // limit boundary: allow if now >= tat - allowance
            let limit = current_tat.saturating_sub(self.burst_allowance_us);
            if now_us < limit {
                return false;
            }

            // next tat: max(now, tat) + tau
            let base = now_us.max(current_tat);
            let next_tat = base.saturating_add(self.emission_interval_us);

            match entry
                .tat_us
                .compare_exchange(current_tat, next_tat, AcqRel, Relaxed)
            {
                Ok(_) => return true,
                Err(next) => current_tat = next,
            }
        }
    }
}

impl GcraRateLimiter {
    /// Creates a new GCRA rate limiter using the system clock.
    ///
    /// # Parameters
    ///
    /// - `rate_per_second`: average rate in requests per second.
    /// - `burst`: maximum burst size allowed.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// - `rate_per_second` is not finite (`NaN` or ±∞).
    /// - `rate_per_second` is not positive (`<= 0.0`).
    /// - `burst` is `0` (must be at least `1`).
    #[inline]
    pub fn new(rate_per_second: f64, burst: u32) -> Self {
        Self::with_time_source(rate_per_second, burst, SystemTimeSource)
    }
}

impl<T: TimeSource> GcraRateLimiter<T> {
    /// Creates a [`GcraRateLimiter`] with a custom [`TimeSource`].
    ///
    /// This is primarily useful for testing and deterministic scenarios.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// - `rate_per_second` is not finite (`NaN` or ±∞).
    /// - `rate_per_second` is not positive (`<= 0.0`).
    /// - `burst` is `0` (must be at least `1`).
    #[inline]
    pub fn with_time_source(rate_per_second: f64, burst: u32, time_source: T) -> Self {
        // Parameter validation
        assert!(rate_per_second.is_finite(), "rate_per_second must be finite");
        assert!(rate_per_second > 0.0, "rate_per_second must be > 0");
        assert!(burst >= 1, "burst must be >= 1");

        // tau_us = ceil(1_000_000 / rate)
        // ceil is conservative: never allows more than a configured rate.
        let tau_f = MICROS_PER_SEC as f64 / rate_per_second;
        let emission_interval_us = tau_f.ceil() as u64;
        let burst_allowance_us = emission_interval_us.saturating_mul((burst - 1) as u64);

        Self {
            storage: Arc::new(DashMap::new()),
            emission_interval_us,
            burst_allowance_us,
            burst,
            eviction_grace_us: DEFAULT_EVICTION, // 60s by default
            time_source,
        }
    }

    /// Sets the eviction grace period for inactive entries.
    ///
    /// Entries that have not been accessed for longer than this duration
    /// may be removed during subsequent `check` calls.
    #[inline]
    pub fn set_eviction(&mut self, eviction: Duration) {
        self.eviction_grace_us = eviction.as_micros()
            .try_into()
            .unwrap_or(u64::MAX);
    }

    /// Average allowed rate in requests per second.
    #[inline(always)]
    pub fn rate_per_second(&self) -> f64 {
        (MICROS_PER_SEC / self.emission_interval_us) as f64
    }

    /// Maximum burst size allowed.
    #[inline(always)]
    pub fn burst(&self) -> u32 {
        self.burst
    }

    /// Time after which inactive entries are eligible for eviction.
    #[inline(always)]
    pub fn eviction_grace_secs(&self) -> u64 {
        self.eviction_grace_us / MICROS_PER_SEC
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::MockTimeSource;

    #[test]
    fn gcra_allows_burst_then_limits() {
        let time = MockTimeSource::new(0);
        let limiter = GcraRateLimiter::with_time_source(1.0, 3, time.clone());
        let key = 10;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));
    }

    #[test]
    fn gcra_refills_over_time() {
        let time = MockTimeSource::new(100);
        let limiter = GcraRateLimiter::with_time_source(1.0, 1, time.clone());
        let key = 5;

        assert!(limiter.check(key));
        assert!(!limiter.check(key));

        time.advance(1);
        assert!(limiter.check(key));
    }

    #[test]
    fn gcra_isolated_per_key() {
        let limiter = GcraRateLimiter::new(1.0, 1);

        assert!(limiter.check(1));
        assert!(!limiter.check(1));
        assert!(limiter.check(2));
    }

    #[test]
    #[should_panic(expected = "rate_per_second must be finite")]
    fn panics_when_rate_is_nan() {
        let _ = GcraRateLimiter::with_time_source(f64::NAN, 1, SystemTimeSource);
    }

    #[test]
    #[should_panic(expected = "rate_per_second must be finite")]
    fn panics_when_rate_is_infinite() {
        let _ = GcraRateLimiter::with_time_source(f64::INFINITY, 1, SystemTimeSource);
    }

    #[test]
    #[should_panic(expected = "rate_per_second must be > 0")]
    fn panics_when_rate_is_zero() {
        let _ = GcraRateLimiter::with_time_source(0.0, 1, SystemTimeSource);
    }

    #[test]
    #[should_panic(expected = "rate_per_second must be > 0")]
    fn panics_when_rate_is_negative() {
        let _ = GcraRateLimiter::with_time_source(-1.0, 1, SystemTimeSource);
    }

    #[test]
    #[should_panic(expected = "burst must be >= 1")]
    fn panics_when_burst_is_zero() {
        let _ = GcraRateLimiter::with_time_source(1.0, 0, SystemTimeSource);
    }
}
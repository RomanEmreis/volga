//! Tools and data structures for a fixed-window rate limiter.

use std::sync::{Arc, atomic::{AtomicU32, AtomicU64, Ordering::Relaxed}};
use std::time::Duration;
use dashmap::DashMap;
use super::{SystemTimeSource, TimeSource, RateLimiter};

/// Internal per-key state for the fixed window algorithm.
///
/// Each entry tracks:
/// - the start timestamp of the current window (in seconds),
/// - the number of requests observed within that window.
///
/// Atomic fields allow concurrent access without global locking.
#[derive(Debug)]
struct Entry {
    /// Number of requests in the current window.
    count: AtomicU32,

    /// A start timestamp (seconds since UNIX_EPOCH) of the current window.
    window_start: AtomicU64,
}

/// A fixed-window rate limiter.
///
/// The fixed window algorithm groups requests into discrete, non-overlapping
/// time windows of a fixed size. For each partition key, a counter is maintained
/// per window and reset when the window changes.
///
/// ## Characteristics
///
/// - **Fast and simple**: O(1) operations with minimal bookkeeping.
/// - **Approximate**: allows bursts at window boundaries.
/// - **Lock-free hot path**: uses atomic counters and concurrent storage.
/// - **Lazy eviction**: stale entries are removed opportunistically.
///
/// ## Algorithm
///
/// For a given `key`:
///
/// 1. The current window is calculated as:
///    `floor(now / window_size) * window_size`.
/// 2. If the stored window differs from the current one, the counter is reset.
/// 3. The request counter is incremented atomically.
/// 4. The request is allowed if the previous counter value was below
///    `max_requests`.
///
/// ## Eviction
///
/// Entries are evicted lazily during `check` calls if they have not been
/// accessed for longer than `eviction_grace_secs`. No background cleanup task
/// is used.
///
/// ## When to use
///
/// This limiter is suitable when:
///
/// - Performance and simplicity are more important than strict accuracy,
/// - occasional bursts at window boundaries are acceptable,
/// - a large number of independent keys is expected.
///
/// For stricter enforcement, consider a sliding window implementation.
#[derive(Debug)]
pub struct FixedWindowRateLimiter<T: TimeSource = SystemTimeSource> {
    /// Per-key rate limiting state.
    storage: Arc<DashMap<u64, Entry>>,

    /// Maximum number of allowed requests per window.
    max_requests: u32,

    /// Size of the fixed window in seconds.
    window_size_secs: u64,

    /// Time after which inactive entries are eligible for eviction.
    ///
    /// This value is independent of `window_size_secs` and is used
    /// solely to limit memory growth.
    eviction_grace_secs: u64,

    /// Time source used to determine the current window.
    time_source: T,
}

impl<T: TimeSource> RateLimiter for FixedWindowRateLimiter<T> {
    /// Checks whether the rate limit has been exceeded for the given `key`.
    ///
    /// Returns `true` if the request is allowed, or `false` if the rate
    /// limit has been reached.
    ///
    /// This operation is lock-free on the hot path and safe for concurrent use.
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now = self.time_source.now_secs();
        let window = self.current_window(now);

        // Lazy eviction
        if let Some(entry) = self.storage.get(&key) {
            let prev_window = entry.window_start.load(Relaxed);
            if now.saturating_sub(prev_window) > self.eviction_grace_secs {
                drop(entry); // release read lock
                self.storage.remove(&key);
            }
        }
        
        let entry = self.storage.entry(key).or_insert_with(|| Entry {
            window_start: AtomicU64::new(window),
            count: AtomicU32::new(0),
        });

        let prev_window = entry.window_start.load(Relaxed);

        // New window -> reset counter
        if prev_window != window {
            entry.window_start.store(window, Relaxed);
            entry.count.store(0, Relaxed);
        }

        let prev = entry.count.fetch_add(1, Relaxed);

        prev < self.max_requests
    }
}

impl FixedWindowRateLimiter {
    /// Creates a new fixed window rate limiter using the system clock.
    ///
    /// # Parameters
    ///
    /// - `max_requests`: maximum number of requests allowed per window.
    /// - `window_size`: duration of a single fixed window.
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self::with_time_source(max_requests, window_size, SystemTimeSource)
    }
}

impl<T: TimeSource> FixedWindowRateLimiter<T> {
    /// Creates a [`FixedWindowRateLimiter`] with a custom [`TimeSource`].
    ///
    /// This is primarily useful for testing or deterministic simulations.
    #[inline]
    pub fn with_time_source(max_requests: u32, window_size: Duration, time_source: T) -> Self {
        let window_size_secs = window_size.as_secs();

        Self {
            storage: Arc::new(DashMap::new()),
            max_requests,
            window_size_secs,
            eviction_grace_secs: window_size_secs * 2,
            time_source,
        }
    }

    /// Sets the eviction grace period for inactive entries.
    ///
    /// Entries that have not been accessed for longer than this duration
    /// may be removed during subsequent `check` calls.
    ///
    /// This method does not perform immediate eviction.
    #[inline]
    pub fn set_eviction(&mut self, eviction: Duration) {
        self.eviction_grace_secs = eviction.as_secs();
    }


    /// Maximum number of allowed requests per window.
    #[inline(always)]
    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// Size of the fixed window in seconds.
    #[inline(always)]
    pub fn window_size_secs(&self) -> u64 {
        self.window_size_secs
    }

    /// Time after which inactive entries are eligible for eviction.
    ///
    /// This value is independent of `window_size_secs` and is used solely to limit memory growth.
    #[inline(always)]
    pub fn eviction_grace_secs(&self) -> u64 {
        self.eviction_grace_secs
    }

    /// Computes the start timestamp of the current window.
    #[inline]
    fn current_window(&self, now: u64) -> u64 {
        (now / self.window_size_secs) * self.window_size_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::MockTimeSource;

    #[test]
    fn fixed_window_allows_within_limit() {
        let limiter = FixedWindowRateLimiter::new(
            3,
            Duration::from_secs(10));
        
        let key = 42;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key)); // 4th denied
    }

    #[test]
    fn fixed_window_resets_after_window() {
        let time = MockTimeSource::new(1000);
        let limiter = FixedWindowRateLimiter::with_time_source(
            2, 
            Duration::from_secs(1),
            time.clone());
        
        let key = 1;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));

        time.advance(1);

        assert!(limiter.check(key)); // new window
    }

    #[test]
    fn fixed_window_isolated_per_key() {
        let limiter = FixedWindowRateLimiter::new(
            1,
            Duration::from_secs(10));

        assert!(limiter.check(1));
        assert!(!limiter.check(1));

        assert!(limiter.check(2)); // independent
    }

    #[test]
    fn fixed_window_is_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(FixedWindowRateLimiter::new(
            1000, 
            Duration::from_secs(10)));
        
        let key = 123;

        let mut handles = vec![];

        for _ in 0..8 {
            let limiter = limiter.clone();
            handles.push(thread::spawn(move || {
                let mut allowed = 0;
                for _ in 0..200 {
                    if limiter.check(key) {
                        allowed += 1;
                    }
                }
                allowed
            }));
        }

        let total: u32 = handles.into_iter().map(|h| h.join().unwrap()).sum();

        // <= limit, possible small race allowance is OK
        assert!(total <= 1000 + 8);
    }
}
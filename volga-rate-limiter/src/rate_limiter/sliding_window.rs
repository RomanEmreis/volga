//! Tools and data structures for a sliding-window rate limiter.

use super::{
    RateLimiter, SystemTimeSource, TimeSource,
    store::{SlidingWindowParams, SlidingWindowStore},
};
use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicU64, Ordering::*},
};
use std::time::Duration;

/// Internal per-key state for the sliding window algorithm.
///
/// The algorithm maintains counters for two adjacent windows:
///
/// - `previous_count`: number of requests in the previous window,
/// - `current_count`: number of requests in the current window.
///
/// The effective request count is calculated as a weighted sum
/// of these two counters, where the weight of the previous window
/// decreases linearly as the current window progresses.
#[derive(Debug)]
struct Entry {
    /// Number of requests in the previous window.
    previous_count: AtomicU32,

    /// Number of requests in the current window.
    current_count: AtomicU32,

    /// A start timestamp (seconds since UNIX_EPOCH) of the current window.
    window_start: AtomicU64,
}

/// In-memory [`SlidingWindowStore`] backed by a concurrent hash map.
///
/// This is the default store used by [`SlidingWindowRateLimiter`].
#[derive(Debug, Clone)]
pub struct InMemorySlidingWindowStore {
    storage: Arc<DashMap<u64, Entry>>,
}

impl InMemorySlidingWindowStore {
    /// Creates a new empty in-memory sliding-window store.
    pub fn new() -> Self {
        Self {
            storage: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemorySlidingWindowStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SlidingWindowStore for InMemorySlidingWindowStore {
    #[inline]
    fn check_and_count(&self, params: SlidingWindowParams) -> bool {
        // Destructuring works here (same crate). External implementors must use params.key etc.
        let SlidingWindowParams {
            key,
            window,
            window_size_secs,
            max_requests,
            now,
            grace_secs,
        } = params;

        // Lazy eviction
        if let Some(entry) = self.storage.get(&key) {
            let window_start = entry.window_start.load(Acquire);
            if now.saturating_sub(window_start) > grace_secs {
                drop(entry);
                self.storage.remove(&key);
            }
        }

        let entry = self.storage.entry(key).or_insert_with(|| {
            let window_start = now / window_size_secs * window_size_secs;
            Entry {
                previous_count: AtomicU32::new(0),
                current_count: AtomicU32::new(0),
                window_start: AtomicU64::new(window_start),
            }
        });

        let window_start = entry.window_start.load(Acquire);

        if window > window_start {
            let windows_passed = (window - window_start) / window_size_secs;

            if windows_passed >= 2 {
                entry.previous_count.store(0, Release);
                entry.current_count.store(0, Release);
                entry.window_start.store(window, Release);
            } else {
                let old_current = entry.current_count.swap(0, AcqRel);
                entry.previous_count.store(old_current, Release);
                entry.window_start.store(window, Release);
            }
        }

        let previous = entry.previous_count.load(Acquire);
        let current = entry.current_count.load(Acquire);

        let elapsed_in_window = now - entry.window_start.load(Acquire);
        let progress = (elapsed_in_window as f64 / window_size_secs as f64).min(1.0);
        let previous_weight = 1.0 - progress;
        let effective = previous as f64 * previous_weight + current as f64;

        if effective >= f64::from(max_requests) {
            return false;
        }

        entry.current_count.fetch_add(1, Release);
        true
    }
}

/// A sliding-window rate limiter.
///
/// Unlike a fixed window, the sliding window algorithm provides a smoother
/// and more accurate rate limiting behavior by accounting for requests
/// from the previous window with a time-based weight.
///
/// ## Characteristics
///
/// - **More accurate** than fixed window rate limiting.
/// - **Reduces boundary bursts** by smoothing request counts.
/// - **Lock-free hot path** using atomic counters.
/// - **Higher computational cost** due to floating-point arithmetic.
///
/// ## Algorithm
///
/// For a given `key`:
///
/// 1. The current fixed window is calculated from the current timestamp.
/// 2. If the window has advanced:
///    - If two or more windows have passed, counters are fully reset.
///    - If exactly one window has passed, the current counter becomes the
///      previous counter.
/// 3. The effective request count is computed as:
///
/// ```text
/// effective = previous_count * (1 - progress) + current_count
/// ```
///
/// where `progress` is the fraction of the current window elapsed
/// in the range `[0.0, 1.0]`.
///
/// 4. The request is allowed if `effective < max_requests`.
///
/// ## Eviction
///
/// Like the fixed window limiter, entries are evicted lazily during `check`
/// calls when they exceed `eviction_grace_secs`.
///
/// ## When to use
///
/// This limiter is appropriate when:
///
/// - burstiness at window boundaries must be minimized,
/// - fairer distribution of requests over time is required,
/// - slightly higher CPU cost is acceptable.
///
/// For maximum throughput and simplicity, consider a fixed window limiter.
#[derive(Debug)]
pub struct SlidingWindowRateLimiter<
    T: TimeSource = SystemTimeSource,
    S: SlidingWindowStore = InMemorySlidingWindowStore,
> {
    store: S,
    max_requests: u32,
    window_size_secs: u64,
    eviction_grace_secs: u64,
    time_source: T,
}

impl<T: TimeSource, S: SlidingWindowStore> RateLimiter for SlidingWindowRateLimiter<T, S> {
    /// Checks whether the rate limit has been exceeded for the given `key`.
    ///
    /// Returns `true` if the request is allowed, or `false` if the rate
    /// limit has been reached.
    ///
    /// This method is safe for concurrent use and performs no global locking.
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now = self.time_source.now_secs();
        let window = now / self.window_size_secs * self.window_size_secs;
        self.store.check_and_count(SlidingWindowParams {
            key,
            window,
            window_size_secs: self.window_size_secs,
            max_requests: self.max_requests,
            now,
            grace_secs: self.eviction_grace_secs,
        })
    }
}

impl SlidingWindowRateLimiter {
    /// Creates a new sliding window rate limiter using the system clock
    /// and the default in-memory store.
    ///
    /// # Parameters
    ///
    /// - `max_requests`: maximum number of requests allowed per window.
    /// - `window_size`: logical duration of the sliding window.
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is less than 1 second.
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self::with_time_source(max_requests, window_size, SystemTimeSource)
    }
}

impl<T: TimeSource> SlidingWindowRateLimiter<T> {
    /// Creates a [`SlidingWindowRateLimiter`] with a custom [`TimeSource`].
    ///
    /// This is primarily useful for testing and deterministic scenarios.
    ///
    /// Note: the previous `T: Clone` bound has been dropped — it was never required
    /// by the algorithm and was an unnecessary constraint.
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is less than 1 second.
    #[inline]
    pub fn with_time_source(max_requests: u32, window_size: Duration, time_source: T) -> Self {
        Self::with_time_source_and_store(
            max_requests,
            window_size,
            time_source,
            InMemorySlidingWindowStore::new(),
        )
    }
}

impl<S: SlidingWindowStore> SlidingWindowRateLimiter<SystemTimeSource, S> {
    /// Creates a [`SlidingWindowRateLimiter`] with a custom [`SlidingWindowStore`].
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is less than 1 second.
    #[inline]
    pub fn with_store(max_requests: u32, window_size: Duration, store: S) -> Self {
        Self::with_time_source_and_store(max_requests, window_size, SystemTimeSource, store)
    }
}

impl<T: TimeSource, S: SlidingWindowStore> SlidingWindowRateLimiter<T, S> {
    /// Creates a [`SlidingWindowRateLimiter`] with a custom [`TimeSource`] and [`SlidingWindowStore`].
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is less than 1 second.
    #[inline]
    pub fn with_time_source_and_store(
        max_requests: u32,
        window_size: Duration,
        time_source: T,
        store: S,
    ) -> Self {
        let window_size_secs = window_size.as_secs();
        assert!(
            window_size_secs > 0,
            "window_size must be at least 1 second"
        );
        Self {
            store,
            max_requests,
            window_size_secs,
            eviction_grace_secs: window_size_secs.saturating_mul(2),
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

    /// Maximum number of allowed requests per window.
    #[inline(always)]
    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// Size of the sliding window in seconds.
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
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::MockTimeSource;
    use super::*;

    #[test]
    fn sliding_window_allows_within_limit() {
        let limiter = SlidingWindowRateLimiter::new(3, Duration::from_secs(10));

        let key = 7;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));
    }

    #[test]
    fn it_tests_window_sliding() {
        let time = MockTimeSource::new(1000);
        let limiter =
            SlidingWindowRateLimiter::with_time_source(10, Duration::from_secs(10), time.clone());

        for i in 0..10 {
            assert!(limiter.check(1), "Request {} should pass", i + 1);
        }
        assert!(!limiter.check(1), "Request 11 should be denied");

        time.advance(5);

        assert!(!limiter.check(1), "Should be denied at 50% of window");

        time.advance(6);

        assert!(limiter.check(1), "Should allow in new window");

        time.advance(10);

        for i in 0..10 {
            assert!(
                limiter.check(1),
                "Request {} should pass after reset",
                i + 1
            );
        }
        assert!(!limiter.check(1), "Request 11 should be denied");
    }

    #[test]
    fn it_tests_window_transition() {
        let time = MockTimeSource::new(2000);
        let limiter =
            SlidingWindowRateLimiter::with_time_source(3, Duration::from_secs(10), time.clone());

        assert!(limiter.check(1));
        assert!(limiter.check(1));
        assert!(limiter.check(1));
        assert!(!limiter.check(1), "4th request should be denied");

        time.advance(5);

        // progress = 5/10 = 0.5, previous_weight = 0.5
        // effective = 0 * 0.5 + 3 = 3.0
        assert!(!limiter.check(1), "Should be denied at 50%");

        time.advance(6);

        // previous = 3, current = 0
        // elapsed_in_window = 2011 - 2010 = 1
        // progress = 1/10 = 0.1, previous_weight = 0.9
        // effective = 3 * 0.9 + 0 = 2.7 < 3
        assert!(limiter.check(1), "Should allow 1st request in new window");

        // current = 1
        // effective = 3 * 0.9 + 1 = 3.7 > 3
        assert!(
            !limiter.check(1),
            "Should be denied - effective = 3*0.9 + 1 = 3.7"
        );

        time.advance(2);

        // elapsed_in_window = 2013 - 2010 = 3
        // progress = 3/10 = 0.3, previous_weight = 0.7
        // effective = 3 * 0.7 + 1 = 3.1 > 3
        assert!(
            !limiter.check(1),
            "Still denied - effective = 3*0.7 + 1 = 3.1"
        );

        time.advance(4);

        // elapsed_in_window = 2017 - 2010 = 7
        // progress = 7/10 = 0.7, previous_weight = 0.3
        // effective = 3 * 0.3 + 1 = 1.9 < 3
        assert!(
            limiter.check(1),
            "Should allow - effective = 3*0.3 + 1 = 1.9"
        );
        assert!(
            limiter.check(1),
            "Should allow - effective = 3*0.3 + 2 = 2.9"
        );
    }

    #[test]
    fn sliding_window_isolated_per_key() {
        let limiter = SlidingWindowRateLimiter::new(1, Duration::from_secs(5));

        assert!(limiter.check(1));
        assert!(!limiter.check(1));

        assert!(limiter.check(2));
    }

    #[test]
    fn sliding_window_with_custom_store_delegates_to_store() {
        use crate::rate_limiter::store::{SlidingWindowParams, SlidingWindowStore};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

        struct CountingStore {
            inner: InMemorySlidingWindowStore,
            calls: Arc<AtomicU32>,
        }
        impl SlidingWindowStore for CountingStore {
            fn check_and_count(&self, params: SlidingWindowParams) -> bool {
                self.calls.fetch_add(1, Relaxed);
                self.inner.check_and_count(params)
            }
        }

        let calls = Arc::new(AtomicU32::new(0));
        let store = CountingStore {
            inner: InMemorySlidingWindowStore::new(),
            calls: calls.clone(),
        };
        let limiter = SlidingWindowRateLimiter::with_store(3, Duration::from_secs(10), store);

        assert!(limiter.check(1));
        assert_eq!(calls.load(Relaxed), 1);
    }

    #[test]
    #[should_panic(expected = "window_size must be at least 1 second")]
    fn sliding_window_panics_on_zero_window_size() {
        let _ = SlidingWindowRateLimiter::new(10, Duration::ZERO);
    }

    #[test]
    fn sliding_window_is_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(SlidingWindowRateLimiter::new(1000, Duration::from_secs(10)));

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

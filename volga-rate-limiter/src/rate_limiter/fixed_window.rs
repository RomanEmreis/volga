//! Tools and data structures for a fixed-window rate limiter.

use super::{
    RateLimiter, SystemTimeSource, TimeSource,
    store::{FixedWindowParams, FixedWindowStore},
};
use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicU64, Ordering::Relaxed},
};
use std::time::Duration;

/// Internal per-key state for the in-memory fixed window store.
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

/// In-memory [`FixedWindowStore`] backed by a concurrent hash map.
///
/// This is the default store used by [`FixedWindowRateLimiter`].
/// It holds per-key counters in a `DashMap` and performs lazy eviction.
#[derive(Debug, Clone)]
pub struct InMemoryFixedWindowStore {
    storage: Arc<DashMap<u64, Entry>>,
}

impl InMemoryFixedWindowStore {
    /// Creates a new empty in-memory fixed-window store.
    pub fn new() -> Self {
        Self { storage: Arc::new(DashMap::new()) }
    }
}

impl Default for InMemoryFixedWindowStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FixedWindowStore for InMemoryFixedWindowStore {
    #[inline]
    fn check_and_count(&self, params: FixedWindowParams) -> bool {
        // Destructuring works here (same crate). External implementors must use params.key etc.
        let FixedWindowParams { key, window, max_requests, now, grace_secs } = params;

        // Lazy eviction
        if let Some(entry) = self.storage.get(&key) {
            let prev_window = entry.window_start.load(Relaxed);
            if now.saturating_sub(prev_window) > grace_secs {
                drop(entry);
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
        prev < max_requests
    }
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
pub struct FixedWindowRateLimiter<
    T: TimeSource = SystemTimeSource,
    S: FixedWindowStore = InMemoryFixedWindowStore,
> {
    store: S,
    max_requests: u32,
    window_size_secs: u64,
    eviction_grace_secs: u64,
    time_source: T,
}

impl<T: TimeSource, S: FixedWindowStore> RateLimiter for FixedWindowRateLimiter<T, S> {
    /// Checks whether the rate limit has been exceeded for the given `key`.
    ///
    /// Returns `true` if the request is allowed, or `false` if the rate
    /// limit has been reached.
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now = self.time_source.now_secs();
        let window = self.current_window(now);
        self.store.check_and_count(FixedWindowParams {
            key,
            window,
            max_requests: self.max_requests,
            now,
            grace_secs: self.eviction_grace_secs,
        })
    }
}

impl FixedWindowRateLimiter {
    /// Creates a new fixed window rate limiter using the system clock
    /// and the default in-memory store.
    ///
    /// # Parameters
    ///
    /// - `max_requests`: maximum number of requests allowed per window.
    /// - `window_size`: duration of a single fixed window.
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is less than 1 second.
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self::with_time_source(max_requests, window_size, SystemTimeSource)
    }
}

impl<T: TimeSource> FixedWindowRateLimiter<T> {
    /// Creates a [`FixedWindowRateLimiter`] with a custom [`TimeSource`].
    ///
    /// This is primarily useful for testing or deterministic simulations.
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
            InMemoryFixedWindowStore::new(),
        )
    }
}

impl<S: FixedWindowStore> FixedWindowRateLimiter<SystemTimeSource, S> {
    /// Creates a [`FixedWindowRateLimiter`] with a custom [`FixedWindowStore`].
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is less than 1 second.
    #[inline]
    pub fn with_store(max_requests: u32, window_size: Duration, store: S) -> Self {
        Self::with_time_source_and_store(max_requests, window_size, SystemTimeSource, store)
    }
}

impl<T: TimeSource, S: FixedWindowStore> FixedWindowRateLimiter<T, S> {
    /// Creates a [`FixedWindowRateLimiter`] with a custom [`TimeSource`] and [`FixedWindowStore`].
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
        assert!(window_size_secs > 0, "window_size must be at least 1 second");
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

    #[inline]
    fn current_window(&self, now: u64) -> u64 {
        (now / self.window_size_secs) * self.window_size_secs
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::MockTimeSource;
    use super::*;

    #[test]
    fn fixed_window_allows_within_limit() {
        let limiter = FixedWindowRateLimiter::new(3, Duration::from_secs(10));

        let key = 42;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key)); // 4th denied
    }

    #[test]
    fn fixed_window_resets_after_window() {
        let time = MockTimeSource::new(1000);
        let limiter =
            FixedWindowRateLimiter::with_time_source(2, Duration::from_secs(1), time.clone());

        let key = 1;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));

        time.advance(1);

        assert!(limiter.check(key)); // new window
    }

    #[test]
    fn fixed_window_isolated_per_key() {
        let limiter = FixedWindowRateLimiter::new(1, Duration::from_secs(10));

        assert!(limiter.check(1));
        assert!(!limiter.check(1));

        assert!(limiter.check(2)); // independent
    }

    #[test]
    fn fixed_window_with_custom_store_allows_within_limit() {
        use crate::rate_limiter::store::{FixedWindowParams, FixedWindowStore};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

        struct CountingStore {
            inner: InMemoryFixedWindowStore,
            calls: Arc<AtomicU32>,
        }
        impl FixedWindowStore for CountingStore {
            fn check_and_count(&self, params: FixedWindowParams) -> bool {
                self.calls.fetch_add(1, Relaxed);
                self.inner.check_and_count(params)
            }
        }

        let calls = Arc::new(AtomicU32::new(0));
        let store = CountingStore {
            inner: InMemoryFixedWindowStore::new(),
            calls: calls.clone(),
        };
        let limiter = FixedWindowRateLimiter::with_store(3, Duration::from_secs(10), store);

        assert!(limiter.check(1));
        assert!(limiter.check(1));
        assert!(limiter.check(1));
        assert!(!limiter.check(1));
        assert_eq!(calls.load(Relaxed), 4);
    }

    #[test]
    #[should_panic(expected = "window_size must be at least 1 second")]
    fn fixed_window_panics_on_zero_window_size() {
        let _ = FixedWindowRateLimiter::new(10, Duration::ZERO);
    }

    #[test]
    fn fixed_window_is_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(FixedWindowRateLimiter::new(1000, Duration::from_secs(10)));

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

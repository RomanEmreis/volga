//! Tools and data structures for sliding window rate limiter

use std::sync::{Arc, atomic::{AtomicU32, AtomicU64, Ordering::*}};
use std::time::Duration;
use dashmap::DashMap;
use super::{SystemTimeSource, TimeSource, RateLimiter};

/// Represents sliding window rate limiting strategy data
#[derive(Debug)]
struct Entry {
    previous_count: AtomicU32,
    current_count: AtomicU32,
    window_start: AtomicU64,
}

/// Represents a sliding window rate limiter
#[derive(Debug)]
pub struct SlidingWindowRateLimiter<T: TimeSource = SystemTimeSource> {
    storage: Arc<DashMap<u64, Entry>>,
    max_requests: u32,
    window_size_secs: u64,
    eviction_grace_secs: u64,
    time_source: T,
}

impl<T: TimeSource> RateLimiter for SlidingWindowRateLimiter<T> {
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now = self.time_source.now_secs();

        // Lazy eviction
        if let Some(entry) = self.storage.get(&key) {
            let window_start = entry.window_start.load(Acquire);
            if now.saturating_sub(window_start) > self.eviction_grace_secs {
                drop(entry); // release read lock
                self.storage.remove(&key);
            }
        }

        let entry = self.storage.entry(key).or_insert_with(|| {
            let window_start = now / self.window_size_secs * self.window_size_secs;
            Entry {
                previous_count: AtomicU32::new(0),
                current_count: AtomicU32::new(0),
                window_start: AtomicU64::new(window_start),
            }
        });

        let window_start = entry.window_start.load(Acquire);
        let current_window = now / self.window_size_secs * self.window_size_secs;

        // If a new window has started, then need to move the counters
        if current_window > window_start {
            let windows_passed = (current_window - window_start) / self.window_size_secs;

            if windows_passed >= 2 {
                // 2+ windows have passed - full reset
                entry.previous_count.store(0, Release);
                entry.current_count.store(0, Release);
                entry.window_start.store(current_window, Release);
            } else {
                // Exactly 1 window has passed - current becomes previous
                let old_current = entry.current_count.swap(0, AcqRel);
                entry.previous_count.store(old_current, Release);
                entry.window_start.store(current_window, Release);
            }
        }

        // Atomic reading of counters to calculate the effective number
        let previous = entry.previous_count.load(Acquire);
        let current = entry.current_count.load(Acquire);

        // Calculate the position in the current window (0.0 = start, 1.0 = end)
        let elapsed_in_window = now - entry.window_start.load(Acquire);
        let progress = (elapsed_in_window as f64 / self.window_size_secs as f64).min(1.0);

        // The weight of the previous window decreases linearly from 1.0 to 0.0
        let previous_weight = 1.0 - progress;

        let effective = previous as f64 * previous_weight + current as f64;

        // Check the limit
        if effective >= self.max_requests as f64 {
            return false;
        }

        // Increment the current counter
        entry.current_count.fetch_add(1, Release);

        true
    }
}

impl SlidingWindowRateLimiter {
    /// Creates a new sliding window rate limiter
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self::with_time_source(max_requests, window_size, SystemTimeSource)
    }
}

impl<T: TimeSource + Clone> SlidingWindowRateLimiter<T> {
    /// Creates a [`SlidingWindowRateLimiter`] with a specific [`TimeSource`]
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

    /// Sets the eviction period
    #[inline]
    pub fn set_eviction(&mut self, eviction: Duration) {
        self.eviction_grace_secs = eviction.as_secs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::MockTimeSource;

    #[test]
    fn sliding_window_allows_within_limit() {
        let limiter = SlidingWindowRateLimiter::new(
            3, 
            Duration::from_secs(10));
        
        let key = 7;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));
    }

    #[test]
    fn it_tests_window_sliding() {
        let time = MockTimeSource::new(1000);
        let limiter = SlidingWindowRateLimiter::with_time_source(
            10, 
            Duration::from_secs(10), 
            time.clone());

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
            assert!(limiter.check(1), "Request {} should pass after reset", i + 1);
        }
        assert!(!limiter.check(1), "Request 11 should be denied");
    }

    #[test]
    fn it_tests_window_transition() {
        let time = MockTimeSource::new(2000);
        let limiter = SlidingWindowRateLimiter::with_time_source(
            3, 
            Duration::from_secs(10), 
            time.clone());

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
        assert!(!limiter.check(1), "Should be denied - effective = 3*0.9 + 1 = 3.7");

        time.advance(2);

        // elapsed_in_window = 2013 - 2010 = 3
        // progress = 3/10 = 0.3, previous_weight = 0.7
        // effective = 3 * 0.7 + 1 = 3.1 > 3
        assert!(!limiter.check(1), "Still denied - effective = 3*0.7 + 1 = 3.1");
        
        time.advance(4);

        // elapsed_in_window = 2017 - 2010 = 7
        // progress = 7/10 = 0.7, previous_weight = 0.3
        // effective = 3 * 0.3 + 1 = 1.9 < 3
        assert!(limiter.check(1), "Should allow - effective = 3*0.3 + 1 = 1.9");
        assert!(limiter.check(1), "Should allow - effective = 3*0.3 + 2 = 2.9");
    }

    #[test]
    fn sliding_window_isolated_per_key() {
        let limiter = SlidingWindowRateLimiter::new(
            1, 
            Duration::from_secs(5));

        assert!(limiter.check(1));
        assert!(!limiter.check(1));

        assert!(limiter.check(2));
    }

    #[test]
    fn sliding_window_is_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(SlidingWindowRateLimiter::new(
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
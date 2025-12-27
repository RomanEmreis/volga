//! Tools and data structures for sliding window rate limiter

use std::{sync::Arc};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::*};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use dashmap::DashMap;
use super::RateLimiter;

/// Represents sliding window rate limiting strategy data
#[derive(Debug)]
struct Entry {
    previous_count: AtomicU32,
    current_count: AtomicU32,
    window_start: AtomicU64,
}

/// Represents a sliding window rate limiter
#[derive(Debug, Clone)]
pub struct SlidingWindowRateLimiter {
    storage: Arc<DashMap<u64, Entry>>,
    max_requests: u32,
    window_size_secs: u64,
    eviction_grace_secs: u64,
}

impl RateLimiter for SlidingWindowRateLimiter {
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now = Self::now_secs();

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
        let window_size_secs = window_size.as_secs();

        Self {
            storage: Arc::new(DashMap::with_capacity(1024)),
            max_requests,
            window_size_secs,
            eviction_grace_secs: window_size_secs * 2,
        }
    }

    #[inline]
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn sliding_window_gradually_allows_requests() {
        let limiter = SlidingWindowRateLimiter::new(
            10, 
            Duration::from_secs(5));

        println!("=== Starting test ===");

        // Заполняем лимит полностью
        for i in 0..10 {
            assert!(limiter.check(1), "Request {} should pass", i + 1);
        }

        // 11-й запрос должен быть отклонен
        assert!(!limiter.check(1), "Request 11 should be denied");

        println!("Made 10 requests, limit reached. Sleeping 2.5 sec (half window)...");
        std::thread::sleep(Duration::from_millis(2500));

        // Через половину окна:
        // progress = 0.5, previous_weight = 0.5
        // effective = 0 * 0.5 + 10 = 10.0 (все еще на лимите)
        if let Some(entry) = limiter.storage.get(&1) {
            println!("After 2.5 sec: prev={}, curr={}",
                     entry.previous_count.load(Acquire),
                     entry.current_count.load(Acquire));
        }
        assert!(!limiter.check(1), "Should still be rate limited at 50% of window");

        println!("Sleeping another 3 sec (total 5.5 sec, new window)...");
        std::thread::sleep(Duration::from_secs(3));

        // Через 5.5+ секунд мы в новом окне
        // previous = 10, current = 0, но мы уже 0.5 сек в новом окне
        // progress = 0.1, previous_weight = 0.9
        // effective = 10 * 0.9 + 0 = 9.0 < 10, должен пройти
        if let Some(entry) = limiter.storage.get(&1) {
            println!("After 5.5 sec (new window): prev={}, curr={}",
                     entry.previous_count.load(Acquire),
                     entry.current_count.load(Acquire));
        }
        assert!(limiter.check(1), "Should allow request in new window");

        println!("Sleeping 6 more sec (past 2 windows)...");
        std::thread::sleep(Duration::from_secs(6));

        // Через 11+ секунд прошло 2+ окна, полный сброс
        if let Some(entry) = limiter.storage.get(&1) {
            println!("After 11+ sec: prev={}, curr={}",
                     entry.previous_count.load(Acquire),
                     entry.current_count.load(Acquire));
        }

        // Должны иметь свежий лимит
        for i in 0..10 {
            assert!(limiter.check(1), "Request {} in fresh window should pass", i + 1);
        }
        assert!(!limiter.check(1), "Request 11 should be denied again");

        println!("=== Test completed ===");

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
}
//! Tools and data structures for fixed window rate limiter

use std::sync::{Arc, atomic::{AtomicU32, AtomicU64, Ordering::Relaxed}};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use dashmap::DashMap;
use super::RateLimiter;

/// Represents fixed window rate limiting strategy data
#[derive(Debug)]
struct Entry {
    count: AtomicU32,
    window_start: AtomicU64,
}

/// Represents a fixed window rate limiter
#[derive(Debug, Clone)]
pub struct FixedWindowRateLimiter {
    storage: Arc<DashMap<u64, Entry>>,
    max_requests: u32,
    window_size_secs: u64,
    eviction_grace_secs: u64,
}

impl RateLimiter for FixedWindowRateLimiter {
    #[inline]
    fn check(&self, key: u64) -> bool {
        let now = Self::now_secs();
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
    /// Creates a new fixed window rate limiter
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        let window_size_secs = window_size.as_secs();

        Self {
            storage: Arc::new(DashMap::with_capacity(1024)),
            max_requests,
            window_size_secs,
            eviction_grace_secs: window_size_secs * 2, // lazy eviction threshold
        }
    }

    #[inline]
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[inline]
    fn current_window(&self, now: u64) -> u64 {
        (now / self.window_size_secs) * self.window_size_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let limiter = FixedWindowRateLimiter::new(
            2, 
            Duration::from_secs(1));
        
        let key = 1;

        assert!(limiter.check(key));
        assert!(limiter.check(key));
        assert!(!limiter.check(key));

        std::thread::sleep(Duration::from_secs(1));

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
//! Generic rate limiter and tools for rate limiting algorithms

use std::time::{SystemTime, UNIX_EPOCH};

pub use fixed_window::FixedWindowRateLimiter;
pub use sliding_window::SlidingWindowRateLimiter;

mod fixed_window;
mod sliding_window;

/// A trait that represents a generic rate limiter
pub trait RateLimiter {
    /// Checks whether the rate limit has been reached for the given partition key
    /// and returns `true` if so, `false` otherwise.
    fn check(&self, key: u64) -> bool;
}

/// A trait for time source
pub trait TimeSource: Send + Sync {
    /// Returns the amount of seconds elapsed from a [`UNIX_EPOCH`]
    /// ("1970-01-01 00:00:00 UTC")
    fn now_secs(&self) -> u64;
}

/// Real time source
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemTimeSource;

impl TimeSource for SystemTimeSource {
    #[inline]
    fn now_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

#[cfg(test)]
pub(super) mod test_utils {
    use std::sync::{Arc, Mutex};
    use super::TimeSource;

    #[derive(Clone)]
    pub(super) struct MockTimeSource {
        current_time: Arc<Mutex<u64>>,
    }

    impl MockTimeSource {
        pub(super) fn new(initial_time: u64) -> Self {
            Self {
                current_time: Arc::new(Mutex::new(initial_time)),
            }
        }

        pub(super) fn advance(&self, seconds: u64) {
            let mut time = self.current_time.lock().unwrap();
            *time += seconds;
        }
    }

    impl TimeSource for MockTimeSource {
        fn now_secs(&self) -> u64 {
            *self.current_time.lock().unwrap()
        }
    }
}
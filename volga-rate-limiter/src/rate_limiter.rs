//! Generic rate limiter abstractions and shared utilities.
//!
//! This module defines the core traits and building blocks used by
//! all rate limiting algorithms provided by this crate.
//!
//! The primary abstraction is [`RateLimiter`], which represents a
//! stateful, thread-safe rate limiting algorithm operating on a
//! partition key.
//!
//! ## Design principles
//!
//! - **Algorithm-agnostic interface** - higher-level frameworks can
//!   work with any rate limiting strategy through a common API.
//! - **Partition-based limiting** - each limiter operates on a `u64`
//!   partition key representing a logical client or request group.
//! - **Time abstraction** - all time-dependent logic is driven by a
//!   pluggable [`TimeSource`] to allow deterministic testing.
//!
//! ## Thread safety
//!
//! All implementations of [`RateLimiter`] provided by this crate are:
//!
//! - Safe to use concurrently
//! - Designed for high-contention scenarios
//! - Intended to be shared between threads and async tasks
//!
//! ## Scope
//!
//! This module does **not** define how partition keys are created or
//! how rate limiting is applied to HTTP requests.
//! Those concerns are intentionally left to higher-level layers.


use std::time::{SystemTime, UNIX_EPOCH};

pub use fixed_window::FixedWindowRateLimiter;
pub use sliding_window::SlidingWindowRateLimiter;
pub use token_bucket::TokenBucketRateLimiter;

mod fixed_window;
mod sliding_window;
mod token_bucket;

/// A generic rate limiter interface.
///
/// A rate limiter tracks request counts per **partition key** and
/// determines whether new requests are allowed.
///
/// Implementations must:
///
/// - Be thread-safe
/// - Handle concurrent access correctly
/// - Execute the `check` operation efficiently, as it is typically
///   called on every incoming request
///
/// The meaning of the partition key is defined by the caller
/// (for example: IP address, user ID, tenant ID, or API key).
pub trait RateLimiter {
    /// Checks whether a request is allowed for the given partition key.
    ///
    /// # Parameters
    ///
    /// - `key`: A stable `u64` value identifying a logical client or
    ///   request group.
    ///
    /// # Returns
    ///
    /// - `true` if the request is allowed and should proceed
    /// - `false` if the rate limit has been exceeded
    ///
    /// # Notes
    ///
    /// - This method may mutate internal state.
    /// - It is expected to be called on the hot path and should be fast.
    fn check(&self, key: u64) -> bool;
}

/// A source of time used by rate-limiting algorithms.
///
/// This abstraction allows rate limiters to be decoupled from
/// the system clock, enabling deterministic and fast unit tests.
pub trait TimeSource: Send + Sync {
    /// Returns the number of seconds elapsed since [`UNIX_EPOCH`]
    /// (`1970-01-01 00:00:00 UTC`).
    ///
    /// Implementations must ensure that the returned value is:
    ///
    /// - Monotonic (non-decreasing)
    /// - Cheap to compute
    fn now_secs(&self) -> u64;
}

/// A [`TimeSource`] implementation based on the system clock.
///
/// This is the default time source used by rate limiters in production.
/// It relies on [`SystemTime`] and returns wall-clock time in seconds.
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
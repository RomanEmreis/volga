//! Generic rate limiter and tools for rate limiting algorithms

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
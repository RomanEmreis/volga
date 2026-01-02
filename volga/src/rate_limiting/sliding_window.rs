//! Tools and structs for a sliding-window rate limiting configuration

use std::time::Duration;
use super::SlidingWindowRateLimiter;

/// Configuration for a **sliding window** rate limiting policy.
///
/// This struct defines the policy parameters:
/// - `max_requests` — maximum number of requests allowed per window
/// - `window_size` — duration of a single sliding window
/// - `eviction` — optional duration after which the data for inactive clients is cleaned up
/// - `name` — optional name to identify a named policy
#[derive(Debug, Clone)]
pub struct SlidingWindow {
    /// Optional name of the policy
    pub(super) name: Option<String>,

    /// Maximum number of requests allowed in the window
    max_requests: u32,

    /// Duration of the window
    window_size: Duration,

    /// Optional eviction period
    eviction: Option<Duration>,
}

impl SlidingWindow {
    /// Creates a new sliding window rate limiting policy.
    ///
    /// # Arguments
    /// * `max_requests` - Maximum number of requests allowed in one window.
    /// * `window_size` - Duration of the window.
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self {
            name: None,
            eviction: None,
            max_requests,
            window_size
        }
    }

    /// Sets an optional eviction period for cleaning up old client state.
    #[inline]
    pub fn with_eviction(mut self, eviction: Duration) -> Self {
        self.eviction = Some(eviction);
        self
    }

    /// Sets the optional name of this policy.
    #[inline]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Builds a `SlidingWindowRateLimiter` instance based on this policy.
    #[inline]
    pub(super) fn build(&self) -> SlidingWindowRateLimiter {
        let mut limiter = SlidingWindowRateLimiter::new(
            self.max_requests,
            self.window_size
        );

        if let Some(eviction) = self.eviction {
            limiter.set_eviction(eviction);
        }

        limiter
    }
}
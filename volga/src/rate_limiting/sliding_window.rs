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


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn it_creates_new_fixed_window_with_basic_parameters() {
        let policy = SlidingWindow::new(100, Duration::from_secs(60));

        assert_eq!(policy.max_requests, 100);
        assert_eq!(policy.window_size, Duration::from_secs(60));
        assert!(policy.name.is_none());
        assert!(policy.eviction.is_none());
    }

    #[test]
    fn it_sets_eviction_period() {
        let policy = SlidingWindow::new(100, Duration::from_secs(60))
            .with_eviction(Duration::from_secs(300));

        assert_eq!(policy.eviction, Some(Duration::from_secs(300)));
    }

    #[test]
    fn it_sets_policy_name_from_string() {
        let policy = SlidingWindow::new(100, Duration::from_secs(60))
            .with_name("api_limiter");

        assert_eq!(policy.name, Some("api_limiter".to_string()));
    }

    #[test]
    fn it_sets_policy_name_from_string_slice() {
        let name = String::from("test_policy");
        let policy = SlidingWindow::new(100, Duration::from_secs(60))
            .with_name(name.clone());

        assert_eq!(policy.name, Some(name));
    }

    #[test]
    fn it_chains_multiple_builder_methods() {
        let policy = SlidingWindow::new(50, Duration::from_secs(30))
            .with_name("chained_policy")
            .with_eviction(Duration::from_secs(600));

        assert_eq!(policy.max_requests, 50);
        assert_eq!(policy.window_size, Duration::from_secs(30));
        assert_eq!(policy.name, Some("chained_policy".to_string()));
        assert_eq!(policy.eviction, Some(Duration::from_secs(600)));
    }

    #[test]
    fn it_builds_rate_limiter_without_eviction() {
        let policy = SlidingWindow::new(100, Duration::from_secs(60));
        let limiter = policy.build();

        assert_eq!(limiter.max_requests(), 100);
        assert_eq!(limiter.window_size_secs(), 60);
        assert_eq!(limiter.eviction_grace_secs(), 60 * 2);
    }

    #[test]
    fn it_builds_rate_limiter_with_eviction() {
        let policy = SlidingWindow::new(100, Duration::from_secs(60))
            .with_eviction(Duration::from_secs(300));
        let limiter = policy.build();

        assert_eq!(limiter.max_requests(), 100);
        assert_eq!(limiter.window_size_secs(), 60);
        assert_eq!(limiter.eviction_grace_secs(), 300);
    }

    #[test]
    fn it_creates_policy_with_zero_max_requests() {
        let policy = SlidingWindow::new(0, Duration::from_secs(60));

        assert_eq!(policy.max_requests, 0);
    }

    #[test]
    fn it_creates_policy_with_very_large_max_requests() {
        let policy = SlidingWindow::new(u32::MAX, Duration::from_secs(1));

        assert_eq!(policy.max_requests, u32::MAX);
    }

    #[test]
    fn it_creates_policy_with_zero_duration_window() {
        let policy = SlidingWindow::new(100, Duration::from_secs(0));

        assert_eq!(policy.window_size, Duration::from_secs(0));
    }

    #[test]
    fn it_creates_policy_with_subsecond_window() {
        let policy = SlidingWindow::new(100, Duration::from_millis(500));

        assert_eq!(policy.window_size, Duration::from_millis(500));
    }

    #[test]
    fn it_clones_policy_correctly() {
        let original = SlidingWindow::new(100, Duration::from_secs(60))
            .with_name("original")
            .with_eviction(Duration::from_secs(300));

        let cloned = original.clone();

        assert_eq!(cloned.max_requests, original.max_requests);
        assert_eq!(cloned.window_size, original.window_size);
        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.eviction, original.eviction);
    }

    #[test]
    fn it_creates_multiple_independent_policies() {
        let policy1 = SlidingWindow::new(100, Duration::from_secs(60))
            .with_name("policy1");
        let policy2 = SlidingWindow::new(200, Duration::from_secs(120))
            .with_name("policy2");

        assert_eq!(policy1.max_requests, 100);
        assert_eq!(policy2.max_requests, 200);
        assert_eq!(policy1.name, Some("policy1".to_string()));
        assert_eq!(policy2.name, Some("policy2".to_string()));
    }

    #[test]
    fn it_overwrites_eviction_when_called_multiple_times() {
        let policy = SlidingWindow::new(100, Duration::from_secs(60))
            .with_eviction(Duration::from_secs(300))
            .with_eviction(Duration::from_secs(600));

        assert_eq!(policy.eviction, Some(Duration::from_secs(600)));
    }

    #[test]
    fn it_overwrites_name_when_called_multiple_times() {
        let policy = SlidingWindow::new(100, Duration::from_secs(60))
            .with_name("first_name")
            .with_name("second_name");

        assert_eq!(policy.name, Some("second_name".to_string()));
    }
}
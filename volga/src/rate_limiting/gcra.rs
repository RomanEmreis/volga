//! Tools and structs for a GCRA (Generic Cell Rate Algorithm) rate limiting configuration

use std::time::Duration;
use super::GcraRateLimiter;

/// Configuration for a [**GCRA (Generic Cell Rate Algorithm)**](https://en.wikipedia.org/wiki/Generic_cell_rate_algorithm) rate limiting policy.
///
/// This struct defines the policy parameters:
/// - `rate_per_second` - average rate in requests per second.
/// - `burst` - maximum burst size allowed.
/// - `eviction` - optional duration after which the data for inactive clients is cleaned up
/// - `name` - optional name to identify a named policy
#[derive(Debug, Clone)]
pub struct Gcra {
    /// Optional name of the policy
    pub(super) name: Option<String>,

    /// Average rate in requests per second.
    rate_per_second: f64,

    /// Configured burst size.
    burst: u32,

    /// Optional eviction period
    eviction: Option<Duration>,
}

impl Gcra {
    /// Creates a new GCRA rate limiting policy.
    ///
    /// # Arguments
    /// - `rate_per_second` - Average rate in requests per second.
    /// - `burst` - Maximum burst size allowed.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// - `rate_per_second` is not finite (`NaN` or ±∞).
    /// - `rate_per_second` is not positive (`<= 0.0`).
    /// - `burst` is `0` (must be at least `1`).
    #[inline]
    pub fn new(rate_per_second: f64, burst: u32) -> Self {
        Self {
            name: None,
            eviction: None,
            rate_per_second,
            burst
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

    /// Builds a `GcraRateLimiter` instance based on this policy.
    #[inline]
    pub(super) fn build(&self) -> GcraRateLimiter {
        let mut limiter = GcraRateLimiter::new(
            self.rate_per_second,
            self.burst
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
    fn it_creates_new_token_bucket_with_basic_parameters() {
        let policy = Gcra::new(1.0, 1);

        assert_eq!(policy.rate_per_second, 1.0);
        assert_eq!(policy.burst, 1);
        assert!(policy.name.is_none());
        assert!(policy.eviction.is_none());
    }

    #[test]
    fn it_sets_eviction_period() {
        let policy = Gcra::new(1.0, 1)
            .with_eviction(Duration::from_secs(300));

        assert_eq!(policy.eviction, Some(Duration::from_secs(300)));
    }

    #[test]
    fn it_sets_policy_name_from_string() {
        let policy = Gcra::new(1.0, 1)
            .with_name("api_limiter");

        assert_eq!(policy.name, Some("api_limiter".to_string()));
    }

    #[test]
    fn it_sets_policy_name_from_string_slice() {
        let name = String::from("test_policy");
        let policy = Gcra::new(1.0, 1)
            .with_name(name.clone());

        assert_eq!(policy.name, Some(name));
    }

    #[test]
    fn it_chains_multiple_builder_methods() {
        let policy = Gcra::new(1.0, 1)
            .with_name("chained_policy")
            .with_eviction(Duration::from_secs(600));

        assert_eq!(policy.rate_per_second, 1.0);
        assert_eq!(policy.burst, 1);
        assert_eq!(policy.name, Some("chained_policy".to_string()));
        assert_eq!(policy.eviction, Some(Duration::from_secs(600)));
    }

    #[test]
    fn it_builds_rate_limiter_without_eviction() {
        let policy = Gcra::new(1.0, 1);
        let limiter = policy.build();

        assert_eq!(limiter.rate_per_second(), 1.0);
        assert_eq!(limiter.burst(), 1);
        assert_eq!(limiter.eviction_grace_secs(), 60);
    }

    #[test]
    fn it_builds_rate_limiter_with_eviction() {
        let policy = Gcra::new(1.0, 1)
            .with_eviction(Duration::from_secs(300));
        let limiter = policy.build();

        assert_eq!(limiter.rate_per_second(), 1.0);
        assert_eq!(limiter.burst(), 1);
        assert_eq!(limiter.eviction_grace_secs(), 300);
    }

    #[test]
    fn it_clones_policy_correctly() {
        let original = Gcra::new(1.0, 1)
            .with_name("original")
            .with_eviction(Duration::from_secs(300));

        let cloned = original.clone();

        assert_eq!(cloned.rate_per_second, original.rate_per_second);
        assert_eq!(cloned.burst, original.burst);
        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.eviction, original.eviction);
    }

    #[test]
    fn it_creates_multiple_independent_policies() {
        let policy1 = Gcra::new(1.0, 1)
            .with_name("policy1");
        let policy2 = Gcra::new(1.0, 3)
            .with_name("policy2");

        assert_eq!(policy1.burst, 1);
        assert_eq!(policy2.burst, 3);
        assert_eq!(policy1.name, Some("policy1".to_string()));
        assert_eq!(policy2.name, Some("policy2".to_string()));
    }

    #[test]
    fn it_overwrites_eviction_when_called_multiple_times() {
        let policy = Gcra::new(1.0, 1)
            .with_eviction(Duration::from_secs(300))
            .with_eviction(Duration::from_secs(600));

        assert_eq!(policy.eviction, Some(Duration::from_secs(600)));
    }

    #[test]
    fn it_overwrites_name_when_called_multiple_times() {
        let policy = Gcra::new(1.0, 1)
            .with_name("first_name")
            .with_name("second_name");

        assert_eq!(policy.name, Some("second_name".to_string()));
    }
}
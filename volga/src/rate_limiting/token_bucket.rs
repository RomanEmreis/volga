//! Tools and structs for a token bucket rate limiting configuration

use std::time::Duration;
use super::TokenBucketRateLimiter;

/// Configuration for a **Token Bucket** rate limiting policy.
///
/// This struct defines the policy parameters:
/// - `capacity` - maximum number of tokens in the bucket.
/// - `refill_rate` - tokens added per second.
/// - `eviction` - optional duration after which the data for inactive clients is cleaned up
/// - `name` - optional name to identify a named policy
#[derive(Debug, Clone)]
pub struct TokenBucket {
    /// Optional name of the policy
    pub(super) name: Option<String>,

    /// Maximum number of tokens in the bucket.
    capacity: u64, 
    
    /// Tokens added per second.
    refill_rate: f64,

    /// Optional eviction period
    eviction: Option<Duration>,
}

impl TokenBucket {
    /// Creates a new token bucket rate limiting policy.
    ///
    /// # Arguments
    /// - `capacity` - Maximum number of tokens in the bucket.
    /// - `refill_rate` - Tokens added per second.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// - `capacity * scale` overflows `u64` when computing the internal fixed-point capacity.
    /// - `refill_rate` is not finite (`NaN` or ±∞).
    /// - `refill_rate` is negative.
    /// - `refill_rate * scale` exceeds `u64::MAX` when computing the internal fixed-point refill rate.
    #[inline]
    pub fn new(capacity: u64, refill_rate: f64) -> Self {
        Self {
            name: None,
            eviction: None,
            capacity,
            refill_rate
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

    /// Builds a `TokenBucketRateLimiter` instance based on this policy.
    #[inline]
    pub(super) fn build(&self) -> TokenBucketRateLimiter {
        let mut limiter = TokenBucketRateLimiter::new(
            self.capacity,
            self.refill_rate
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
        let policy = TokenBucket::new(100, 1.0);

        assert_eq!(policy.capacity, 100);
        assert_eq!(policy.refill_rate, 1.0);
        assert!(policy.name.is_none());
        assert!(policy.eviction.is_none());
    }

    #[test]
    fn it_sets_eviction_period() {
        let policy = TokenBucket::new(100, 1.0)
            .with_eviction(Duration::from_secs(300));

        assert_eq!(policy.eviction, Some(Duration::from_secs(300)));
    }

    #[test]
    fn it_sets_policy_name_from_string() {
        let policy = TokenBucket::new(100, 1.0)
            .with_name("api_limiter");

        assert_eq!(policy.name, Some("api_limiter".to_string()));
    }

    #[test]
    fn it_sets_policy_name_from_string_slice() {
        let name = String::from("test_policy");
        let policy = TokenBucket::new(100, 1.0)
            .with_name(name.clone());

        assert_eq!(policy.name, Some(name));
    }

    #[test]
    fn it_chains_multiple_builder_methods() {
        let policy = TokenBucket::new(50, 2.0)
            .with_name("chained_policy")
            .with_eviction(Duration::from_secs(600));

        assert_eq!(policy.capacity, 50);
        assert_eq!(policy.refill_rate, 2.0);
        assert_eq!(policy.name, Some("chained_policy".to_string()));
        assert_eq!(policy.eviction, Some(Duration::from_secs(600)));
    }

    #[test]
    fn it_builds_rate_limiter_without_eviction() {
        let policy = TokenBucket::new(100, 1.0);
        let limiter = policy.build();

        assert_eq!(limiter.capacity(), 100);
        assert_eq!(limiter.refill_rate(), 1.0);
        assert_eq!(limiter.eviction_grace_secs(), 60);
    }

    #[test]
    fn it_builds_rate_limiter_with_eviction() {
        let policy = TokenBucket::new(100, 1.0)
            .with_eviction(Duration::from_secs(300));
        let limiter = policy.build();

        assert_eq!(limiter.capacity(), 100);
        assert_eq!(limiter.refill_rate(), 1.0);
        assert_eq!(limiter.eviction_grace_secs(), 300);
    }

    #[test]
    fn it_creates_policy_with_zero_capacity() {
        let policy = TokenBucket::new(0, 1.0);

        assert_eq!(policy.capacity, 0);
    }

    #[test]
    fn it_clones_policy_correctly() {
        let original = TokenBucket::new(100, 1.0)
            .with_name("original")
            .with_eviction(Duration::from_secs(300));

        let cloned = original.clone();

        assert_eq!(cloned.capacity, original.capacity);
        assert_eq!(cloned.refill_rate, original.refill_rate);
        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.eviction, original.eviction);
    }

    #[test]
    fn it_creates_multiple_independent_policies() {
        let policy1 = TokenBucket::new(100, 1.0)
            .with_name("policy1");
        let policy2 = TokenBucket::new(200, 2.0)
            .with_name("policy2");

        assert_eq!(policy1.capacity, 100);
        assert_eq!(policy2.capacity, 200);
        assert_eq!(policy1.name, Some("policy1".to_string()));
        assert_eq!(policy2.name, Some("policy2".to_string()));
    }

    #[test]
    fn it_overwrites_eviction_when_called_multiple_times() {
        let policy = TokenBucket::new(100, 1.0)
            .with_eviction(Duration::from_secs(300))
            .with_eviction(Duration::from_secs(600));

        assert_eq!(policy.eviction, Some(Duration::from_secs(600)));
    }

    #[test]
    fn it_overwrites_name_when_called_multiple_times() {
        let policy = TokenBucket::new(100, 2.0)
            .with_name("first_name")
            .with_name("second_name");

        assert_eq!(policy.name, Some("second_name".to_string()));
    }
}
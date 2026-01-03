//! Tools, structs, and traits for rate-limiting partition keys

use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use crate::error::Error;
use crate::HttpRequest;

/// Defines how a rate-limiting partition key is extracted from an HTTP request.
///
/// Implementations of this trait determine how requests are grouped
/// for the purposes of rate limiting.
///
/// The extracted key must be:
/// - Stable for the same logical client
/// - Fast to compute
/// - Safe to use concurrently
pub trait RateLimitKey: Send + Sync {
    /// Extracts a partition key from the given HTTP request.
    ///
    /// Implementations should return a stable `u64` value that uniquely
    /// identifies a client or logical request group.
    fn extract(&self, req: &HttpRequest) -> Result<u64, Error>;
}

/// Extension methods for rate-limiting partition keys.
///
/// This trait provides a fluent API for attaching additional
/// rate-limiting configuration to a [`RateLimitKey`], such as selecting
/// a specific policy.
///
/// It is automatically implemented for all types that implement
/// [`RateLimitKey`], and is intended to be used through the routing DSL.
///
/// # Examples
///
/// ```no_run
/// use volga::{App, rate_limiting::{RateLimitKeyExt, SlidingWindow, by}};
/// use std::time::Duration;
///
/// let mut app = App::new()
///     .with_sliding_window(SlidingWindow::new(10, Duration::from_secs(10)))
///     .with_sliding_window(SlidingWindow::new(10, Duration::from_secs(10)));
///
/// app.map_get("/api", || async {})
///     .sliding_window(by::ip())
///     .sliding_window(by::header("x-tenant-id").using("burst"));
/// ```
///
/// The example above applies two independent rate-limiting policies:
/// one partitioned by IP address and another partitioned by user identity
/// using the `"burst"` policy.
pub trait RateLimitKeyExt: Sized + RateLimitKey + 'static {
    /// Converts this partition key into a rate-limiting binding
    /// using the default policy for the selected algorithm.
    ///
    /// This method is typically called implicitly by the routing DSL
    /// and does not need to be invoked directly by users.
    fn bind(self) -> RateLimitBinding;
}

impl RateLimitKey for RateLimitBinding {
    #[inline]
    fn extract(&self, req: &HttpRequest) -> Result<u64, Error> {
        self.key.extract(req)
    }
}

impl RateLimitKeyExt for RateLimitBinding {
    #[inline]
    fn bind(self) -> RateLimitBinding {
        self
    }
}

/// A symbolic name of a rate-limiting policy.
///
/// Policy names are used to select a concrete rate-limiting configuration
/// (e.g. limits, window size, eviction behavior) that was previously
/// registered on the application.
///
/// Internally, this type is reference-counted to allow cheap cloning
/// when rate-limiting bindings are shared across routes or middleware.
///
/// Typical examples include `"default"`, `"burst"`, or `"strict"`.
pub type PolicyName = Arc<str>;

/// A fully configured rate-limiting binding.
///
/// A binding combines:
/// - a [`RateLimitKey`] that defines how requests are partitioned
/// - an optional policy name that selects a rate-limiting configuration
///
/// Bindings are created implicitly by the routing DSL and are not meant
#[derive(Clone)]
pub struct RateLimitBinding {
    /// The partition key extractor used to group requests.
    pub(super) key: Arc<dyn RateLimitKey>,

    /// Optional policy name selecting a specific rate-limiting configuration.
    ///
    /// If `None`, the default policy for the given rate-limiting algorithm
    /// is used.
    pub(super) policy: Option<PolicyName>,
}

impl Debug for RateLimitBinding {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitBinding(...)").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use hyper::Request;
    use crate::HttpBody;

    // Mock implementation of RateLimitKey for testing
    struct MockKey {
        value: u64,
    }

    impl RateLimitKey for MockKey {
        fn extract(&self, _req: &HttpRequest) -> Result<u64, Error> {
            Ok(self.value)
        }
    }

    // Mock implementation that returns an error
    struct ErrorKey;

    impl RateLimitKey for ErrorKey {
        fn extract(&self, _req: &HttpRequest) -> Result<u64, Error> {
            Err(Error::server_error("Mock error"))
        }
    }

    // Helper function to create a binding with a specific key and policy
    fn create_binding_with_policy(
        key: Arc<dyn RateLimitKey>,
        policy: Option<PolicyName>,
    ) -> RateLimitBinding {
        RateLimitBinding { key, policy }
    }

    fn create_request() -> HttpRequest {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        HttpRequest::from_parts(parts, body)
    }

    #[test]
    fn it_extracts_key_from_mock_implementation() {
        let key = MockKey { value: 42 };
        let req = create_request();

        let result = key.extract(&req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn it_propagates_extraction_errors() {
        let key = ErrorKey;
        let req = create_request();

        let result = key.extract(&req);
        assert!(result.is_err());
    }

    #[test]
    fn it_creates_binding_without_policy_name() {
        let key = Arc::new(MockKey { value: 200 });
        let binding = create_binding_with_policy(key, None);

        assert!(binding.policy.is_none());
    }

    #[test]
    fn it_creates_binding_with_policy_name() {
        let key = Arc::new(MockKey { value: 100 });
        let policy_name: Arc<str> = Arc::from("burst");
        let binding = create_binding_with_policy(key, Some(policy_name.clone()));

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "burst");
    }

    #[test]
    fn it_extracts_key_through_binding() {
        let key = Arc::new(MockKey { value: 123 });
        let binding = create_binding_with_policy(key, Some(Arc::from("test_policy")));
        let req = create_request();

        let result = binding.extract(&req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 123);
    }

    #[test]
    fn it_propagates_errors_through_binding() {
        let key = Arc::new(ErrorKey);
        let binding = create_binding_with_policy(key, Some(Arc::from("test_policy")));
        let req = create_request();

        let result = binding.extract(&req);
        assert!(result.is_err());
    }

    #[test]
    fn it_clones_binding_correctly() {
        let key = Arc::new(MockKey { value: 300 });
        let binding = create_binding_with_policy(key, Some(Arc::from("original_policy")));

        let cloned = binding.clone();

        assert!(cloned.policy.is_some());
        assert_eq!(
            cloned.policy.as_ref().unwrap().as_ref(),
            "original_policy"
        );
    }

    #[test]
    fn it_clones_binding_with_shared_policy_reference() {
        let key = Arc::new(MockKey { value: 400 });
        let policy: Arc<str> = Arc::from("shared_policy");
        let binding = create_binding_with_policy(key, Some(policy));

        let cloned = binding.clone();

        // Both should point to the same Arc
        assert!(Arc::ptr_eq(
            binding.policy.as_ref().unwrap(),
            cloned.policy.as_ref().unwrap()
        ));
    }

    #[test]
    fn it_creates_multiple_independent_bindings() {
        let key1 = Arc::new(MockKey { value: 100 });
        let key2 = Arc::new(MockKey { value: 200 });

        let binding1 = create_binding_with_policy(key1, Some(Arc::from("policy1")));
        let binding2 = create_binding_with_policy(key2, Some(Arc::from("policy2")));

        let req = create_request();

        assert_eq!(binding1.extract(&req).unwrap(), 100);
        assert_eq!(binding2.extract(&req).unwrap(), 200);
        assert_eq!(binding1.policy.as_ref().unwrap().as_ref(), "policy1");
        assert_eq!(binding2.policy.as_ref().unwrap().as_ref(), "policy2");
    }

    #[test]
    fn it_binds_binding_to_itself() {
        let key = Arc::new(MockKey { value: 500 });
        let binding = create_binding_with_policy(key, Some(Arc::from("test_policy")));

        let bound = binding.clone().bind();

        assert!(bound.policy.is_some());
        assert_eq!(bound.policy.as_ref().unwrap().as_ref(), "test_policy");
    }

    #[test]
    fn it_extracts_same_value_multiple_times() {
        let key = Arc::new(MockKey { value: 777 });
        let binding = create_binding_with_policy(key, Some(Arc::from("consistent")));
        let req = create_request();

        let result1 = binding.extract(&req);
        let result2 = binding.extract(&req);

        assert_eq!(result1.unwrap(), 777);
        assert_eq!(result2.unwrap(), 777);
    }

    #[test]
    fn it_handles_empty_policy_name() {
        let key = Arc::new(MockKey { value: 999 });
        let binding = create_binding_with_policy(key, Some(Arc::from("")));

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "");
    }

    #[test]
    fn it_handles_long_policy_name() {
        let key = Arc::new(MockKey { value: 111 });
        let long_name = "very_long_policy_name_that_might_be_used_in_real_scenarios";
        let binding = create_binding_with_policy(key, Some(Arc::from(long_name)));

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), long_name);
    }

    #[test]
    fn it_formats_binding_debug_output() {
        let key = Arc::new(MockKey { value: 222 });
        let binding = create_binding_with_policy(key, Some(Arc::from("debug_test")));

        let debug_str = format!("{:?}", binding);
        assert!(debug_str.contains("RateLimitBinding"));
    }

    #[test]
    fn it_shares_key_across_multiple_bindings() {
        let key = Arc::new(MockKey { value: 333 });

        let binding1 = create_binding_with_policy(key.clone(), Some(Arc::from("policy1")));
        let binding2 = create_binding_with_policy(key.clone(), Some(Arc::from("policy2")));

        let req = create_request();

        assert_eq!(binding1.extract(&req).unwrap(), 333);
        assert_eq!(binding2.extract(&req).unwrap(), 333);
    }

    #[test]
    fn it_allows_none_policy_in_binding_construction() {
        let key = Arc::new(MockKey { value: 444 });
        let binding = create_binding_with_policy(key, None);

        assert!(binding.policy.is_none());
    }

    #[test]
    fn it_implements_send_and_sync_for_mock_key() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MockKey>();
    }

    #[test]
    fn it_implements_send_and_sync_for_binding() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RateLimitBinding>();
    }

    #[test]
    fn it_extracts_key_after_bind() {
        let key = Arc::new(MockKey { value: 888 });
        let binding = create_binding_with_policy(key, Some(Arc::from("policy")));
        let req = create_request();

        let bound = binding.bind();
        let result = bound.extract(&req);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 888);
    }

    #[test]
    fn it_preserves_policy_after_bind() {
        let key = Arc::new(MockKey { value: 555 });
        let policy_name: Arc<str> = Arc::from("preserved");
        let binding = create_binding_with_policy(key, Some(policy_name.clone()));

        let bound = binding.bind();

        assert!(bound.policy.is_some());
        assert!(Arc::ptr_eq(bound.policy.as_ref().unwrap(), &policy_name));
    }

    #[test]
    fn it_clones_binding_with_none_policy() {
        let key = Arc::new(MockKey { value: 666 });
        let binding = create_binding_with_policy(key, None);

        let cloned = binding.clone();

        assert!(cloned.policy.is_none());
    }

    #[test]
    fn it_handles_multiple_binds_on_same_binding() {
        let key = Arc::new(MockKey { value: 999 });
        let binding = create_binding_with_policy(key, Some(Arc::from("multi_bind")));

        let bound1 = binding.clone().bind();
        let bound2 = binding.clone().bind();

        assert_eq!(
            bound1.policy.as_ref().unwrap().as_ref(),
            "multi_bind"
        );
        assert_eq!(
            bound2.policy.as_ref().unwrap().as_ref(),
            "multi_bind"
        );
    }
}
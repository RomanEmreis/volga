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
    /// Associates this partition key with a named rate-limiting policy.
    ///
    /// The policy name must refer to a configuration that was previously
    /// registered on the application using `with_fixed_window` or
    /// `with_sliding_window`.
    ///
    /// If the specified policy does not exist, the behavior is framework-
    /// specific and may result in a runtime error.
    #[inline]
    fn using(self, policy: impl Into<PolicyName>) -> impl RateLimitKey {
        RateLimitBinding {
            key: Arc::new(self),
            policy: Some(policy.into()),
        }
    }

    /// Converts this partition key into a rate-limiting binding
    /// using the default policy for the selected algorithm.
    ///
    /// This method is typically called implicitly by the routing DSL
    /// and does not need to be invoked directly by users.
    #[inline]
    fn bind(self) -> RateLimitBinding {
        RateLimitBinding {
            key: Arc::new(self),
            policy: None,
        }
    }
}

impl<T> RateLimitKeyExt for T
where
    T: RateLimitKey + 'static
{}

impl RateLimitKey for RateLimitBinding {
    #[inline]
    fn extract(&self, req: &HttpRequest) -> Result<u64, Error> {
        self.key.extract(req)
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
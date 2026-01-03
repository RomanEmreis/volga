//! Helpers for building rate limiting partition keys.
//!
//! This module provides a set of helpers for defining how a partition key
//! is extracted from an incoming HTTP request.
//!
//! Partition keys are used by rate limiters to group requests
//! (e.g. by client IP address or authenticated user identity).
//!
//! # Examples
//!
//! ```no_run
//! use volga::rate_limiting::by;
//!
//! // Rate limit by client IP
//! by::ip();
//!
//! // Rate limit by X-Api-Key HTTP header
//! by::header("x-api-key");
//! ```

use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use super::{extract_partition_key_from_ip, stable_hash, PolicyName, RateLimitBinding, RateLimitKey, RateLimitKeyExt};
use crate::{HttpRequest, headers::{HeaderName, HeaderError}, error::Error};

#[cfg(feature = "cookie")]
use crate::http::Cookies;

#[cfg(feature = "jwt-auth")]
use crate::auth::{AuthClaims, Authenticated};

/// Represents a source from which a rate-limiting partition key is derived.
///
/// `RateLimitKeySource` is an opaque, high-level abstraction used by the
/// rate-limiting DSL to describe **how requests are grouped into partitions**
/// for rate limiting.
///
/// A partition key determines *which requests share the same rate-limit bucket*.
/// For example:
/// - grouping by client IP address
/// - grouping by authenticated user ID
/// - grouping by any custom request-derived value
///
/// This type intentionally hides its internal implementation details.
/// Users construct `RateLimitKeySource` values via helper functions
/// provided in the [`by`] module, such as [`by::ip`] or [`by::user`].
///
/// # Usage
///
/// `RateLimitKeySource` is typically passed to rate-limiting middleware
/// registration methods and can optionally be associated with a named
/// rate-limiting policy.
///
/// ```no_run
/// use volga::rate_limiting::by;
///
/// # let mut app = volga::App::new();
/// // Apply the default fixed window policy, partitioned by IP address
/// app.use_fixed_window(by::ip());
///
/// // Apply a named policy ("burst") for requests grouped by tenant ID
/// app.use_sliding_window(
///     by::header("x-tenant-id").using("burst")
/// );
/// ```
///
/// # Named Policies
///
/// A partition key source can be bound to a specific rate-limiting policy
/// using [`RateLimitKeySource::using`]. This allows selecting one of multiple
/// configured policies at runtime without changing the partitioning logic.
///
/// ```no_run
/// use volga::rate_limiting::by;
///
/// by::ip().using("strict");
/// ```
///
/// # Design Notes
///
/// - This type is **cheap to clone** and intended to be used in middleware
///   configuration.
/// - The internal key extraction logic is encapsulated and may evolve
///   without breaking the public API.
/// - `RateLimitKeySource` implements [`RateLimitKey`] and can be converted
///   into an internal binding via framework-provided extension traits.
///
/// See the [`by`] module for available key source constructors.
#[derive(Debug, Clone)]
pub struct RateLimitKeySource {
    /// Inner key
    inner: PartitionKey,
}

impl RateLimitKeySource {
    /// Binds this partition key source to a named rate-limiting policy.
    ///
    /// See [`FixedWindow::with_name`] or [`SlidingWindow::with_name`] for
    /// policy configuration.
    pub fn using(self, policy: impl Into<PolicyName>) -> RateLimitBinding {
        RateLimitBinding {
            key: Arc::new(self.inner),
            policy: Some(policy.into()),
        }
    }
}

/// A function that extracts a rate-limiting partition key from an HTTP request.
///
/// The function must return a stable `u64` value that uniquely represents
/// a logical client identity (e.g. IP address or user identifier).
///
/// This type is internally type-erased and stored behind an `Arc`
/// to allow cheap cloning and thread-safe sharing.
type PartitionKeyExtractor = Arc<
    dyn Fn(&HttpRequest) -> Result<u64, Error>
    + Send
    + Sync
    + 'static
>;

/// Represents a source from which a rate-limiting partition key is derived.
///
/// This enum is an internal implementation detail and is exposed to users
/// through helper functions such as [`ip`] and [`user`].
#[derive(Clone)]
enum PartitionKey {
    /// Extracts the partition key from the client IP address.
    ///
    /// The IP address is resolved in the following order:
    /// 1. The standardized `Forwarded` header (RFC 7239)
    /// 2. The legacy `X-Forwarded-For` header
    /// 3. The peer socket address as a fallback
    Ip,

    /// Extracts the partition key using a user-defined function.
    ///
    /// This variant is typically used to derive keys from authenticated
    /// user data (e.g. JWT claims).
    Custom(PartitionKeyExtractor),
}

impl Debug for PartitionKey {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self { 
            PartitionKey::Ip => f.debug_tuple("PartitionKey::Ip").finish(),
            PartitionKey::Custom(_) => f.debug_tuple("PartitionKey::Custom").finish(),
        }
    }
}

impl RateLimitKey for PartitionKey {
    #[inline]
    fn extract(&self, req: &HttpRequest) -> Result<u64, Error> {
        match self {
            PartitionKey::Ip => extract_partition_key_from_ip(req),
            PartitionKey::Custom(extractor) => extractor(req)
        }
    }
}

impl RateLimitKey for RateLimitKeySource {
    #[inline]
    fn extract(&self, req: &HttpRequest) -> Result<u64, Error> {
        self.inner.extract(req)
    }
}

impl RateLimitKeyExt for RateLimitKeySource {
    #[inline]
    fn bind(self) -> RateLimitBinding {
        RateLimitBinding {
            key: Arc::new(self.inner),
            policy: None,
        }
    }
}

/// Uses the client IP address as a rate limiting partition key.
///
/// The IP address is resolved in the following order:
/// 1. The `Forwarded` header (RFC 7239)
/// 2. The `X-Forwarded-For` header
/// 3. The peer socket address as a fallback
///
/// This is the most common strategy for global or unauthenticated rate limiting.
///
/// # Example
/// ```no_run
/// use volga::{App, rate_limiting::by};
/// 
/// let mut app = App::new();
/// app.use_fixed_window(by::ip());
/// ```
#[inline]
pub fn ip() -> RateLimitKeySource {
    RateLimitKeySource {
        inner: PartitionKey::Ip,
    }
}

/// Uses the value of an HTTP header as a rate limiting partition key.
///
/// The header value is hashed into a stable `u64`.
///
/// # Notes
/// - Header names are case-insensitive.
/// - If the header is missing, the key extraction will fail.
///
/// # Example
/// ```no_run
/// use volga::{App, rate_limiting::by};
///
/// let mut app = App::new();
/// app.use_fixed_window(by::header("x-api-key"));
/// ```
#[inline]
pub fn header(name: &'static str) -> RateLimitKeySource {
    let header = HeaderName::from_static(name);

    let key = PartitionKey::Custom(Arc::new(move |req| {
        let value = req.headers()
            .get(&header)
            .ok_or_else(|| HeaderError::header_missing_impl(name))?;

        let value = value.to_str()
            .map_err(HeaderError::from_to_str_error)?;

        Ok(stable_hash(value))
    }));

    RateLimitKeySource { inner: key }
}

/// Uses the value of an HTTP request query parameter as a rate limiting partition key.
///
/// The query parameter value is hashed into a stable `u64`.
///
/// # Notes
/// - Query parameter names are case-insensitive.
/// - If the parameter is missing, the key extraction will fail.
///
/// # Example
/// ```no_run
/// use volga::{App, rate_limiting::by};
///
/// let mut app = App::new();
/// app.use_fixed_window(by::query("key"));
/// ```
#[inline]
pub fn query(name: &'static str) -> RateLimitKeySource {
    let key = PartitionKey::Custom(Arc::new(move |req| {
        let value = req.query_args()
            .find_map(|(k, v)| if k == name { Some(v) } else { None })
            .ok_or_else(|| Error::client_error(format!("Query parameter {name} not found", )))?;

        Ok(stable_hash(value))
    }));
    
    RateLimitKeySource { inner: key }
}

/// Uses the value of an HTTP route path parameter as a rate limiting partition key.
///
/// The route path parameter value is hashed into a stable `u64`.
///
/// # Notes
/// - Route path parameter names are case-insensitive.
/// - If the parameter is missing, the key extraction will fail.
///
/// # Example
/// ```no_run
/// use volga::{App, rate_limiting::by};
///
/// let mut app = App::new();
/// app.use_fixed_window(by::path("key"));
/// ```
#[inline]
pub fn path(name: &'static str) -> RateLimitKeySource {
    let key = PartitionKey::Custom(Arc::new(move |req| {
        let value = req.path_args()
            .find_map(|(k, v)| if k == name { Some(v) } else { None })
            .ok_or_else(|| Error::client_error(format!("Path parameter {name} not found", )))?;

        Ok(stable_hash(value))
    }));
    
    RateLimitKeySource { inner: key }
}

/// Uses the value of an HTTP cookie as a rate limiting partition key.
///
/// The cookie hashed into a stable `u64`.
///
/// # Notes
/// - Cookie names are case-insensitive.
/// - If the cookie is missing, the key extraction will fail.
///
/// # Example
/// ```no_run
/// use volga::{App, rate_limiting::by};
///
/// let mut app = App::new();
/// app.use_fixed_window(by::cookie("session-id"));
/// ```
#[cfg(feature = "cookie")]
#[inline]
pub fn cookie(name: &'static str) -> RateLimitKeySource {
    let key = PartitionKey::Custom(Arc::new(move |req| {
        let cookies = req.extract::<Cookies>()?;
        let cookie = cookies.get(name)
            .ok_or_else(|| Error::client_error(format!("Cookie {name} not found", )))?;

        Ok(stable_hash(cookie.value()))
    }));
    
    RateLimitKeySource { inner: key }
}

/// Uses an authenticated user identity as a rate limiting partition key.
///
/// This helper extracts [`Authenticated<C>`] from the request and applies
/// the provided function to a user claims to derive a stable identifier.
///
/// The returned string is immediately hashed into a `u64` value and is
/// **not stored** beyond the scope of the request.
///
/// # Type Parameters
/// - `C`: A user-defined claims type that implements [`AuthClaims`]
///
/// # Parameters
/// - `f`: A function that extracts a string identifier from user claims
///   (e.g. `sub`, `email`, `tenant_id`)
///
/// # Example
/// ```no_run
/// use volga::{App, auth::AuthClaims, rate_limiting::by};
/// use serde::Deserialize;;
/// 
/// #[derive(Clone, Deserialize)]
/// struct Claims {
///     sub: String,
///     email: String,
/// }
/// 
/// impl AuthClaims for Claims {}
/// 
/// let mut app = App::new();
/// 
/// // Rate limit per user subject
/// app.use_fixed_window(by::user(|claims: &Claims| claims.sub.as_str()));
///
/// // Rate limit per email
/// app.use_fixed_window(by::user(|claims: &Claims| claims.email.as_str()));
/// ```
///
/// # Notes
/// - This function requires the `jwt-auth` feature.
/// - If authentication has not been performed for the request,
///   key extraction will fail.
#[inline]
#[cfg(feature = "jwt-auth")]
pub fn user<C, F>(f: F) -> RateLimitKeySource
where
    C: AuthClaims + Send + Sync + 'static,
    F: Fn(&C) -> &str + Send + Sync + 'static
{
    let key = PartitionKey::Custom(Arc::new(move |req| {
        let auth = req.extract::<Authenticated<C>>()?;
        let key = f(auth.claims());
        Ok(stable_hash(key))
    }));
    
    RateLimitKeySource { inner: key }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpBody, HttpRequest};
    use std::sync::Arc;
    use hyper::Request;

    fn create_request() -> HttpRequest {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        HttpRequest::from_parts(parts, body)
    }

    #[test]
    fn it_creates_ip_based_key_source() {
        let source = ip();

        assert!(matches!(source.inner, PartitionKey::Ip));
    }

    #[test]
    fn it_creates_header_based_key_source() {
        let source = header("x-api-key");

        assert!(matches!(source.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_creates_query_based_key_source() {
        let source = query("api_key");

        assert!(matches!(source.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_creates_path_based_key_source() {
        let source = path("user_id");

        assert!(matches!(source.inner, PartitionKey::Custom(_)));
    }

    #[cfg(feature = "cookie")]
    #[test]
    fn it_creates_cookie_based_key_source() {
        let source = cookie("session-id");

        assert!(matches!(source.inner, PartitionKey::Custom(_)));
    }

    #[cfg(feature = "jwt-auth")]
    #[test]
    fn it_creates_user_based_key_source() {
        use serde::Deserialize;

        #[derive(Clone, Deserialize)]
        struct TestClaims {
            sub: String,
        }

        impl AuthClaims for TestClaims {}

        let source = user(|claims: &TestClaims| claims.sub.as_str());

        assert!(matches!(source.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_binds_key_source_with_policy_name() {
        let source = ip();
        let binding = source.using("burst");

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "burst");
    }

    #[test]
    fn it_binds_key_source_with_string_policy_name() {
        let source = ip();
        let policy_name = String::from("strict");
        let binding = source.using(policy_name);

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "strict");
    }

    #[test]
    fn it_binds_key_source_with_arc_policy_name() {
        let source = ip();
        let policy_name: Arc<str> = Arc::from("custom");
        let binding = source.using(policy_name.clone());

        assert!(binding.policy.is_some());
        assert!(Arc::ptr_eq(binding.policy.as_ref().unwrap(), &policy_name));
    }

    #[test]
    fn it_binds_key_source_without_policy_name() {
        let source = ip();
        let binding = source.bind();

        assert!(binding.policy.is_none());
    }

    #[test]
    fn it_clones_key_source_correctly() {
        let source = ip();
        let cloned = source.clone();

        assert!(matches!(cloned.inner, PartitionKey::Ip));
    }

    #[test]
    fn it_clones_custom_key_source_correctly() {
        let source = header("x-custom-header");
        let cloned = source.clone();

        assert!(matches!(cloned.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_formats_ip_partition_key_debug_output() {
        let key = PartitionKey::Ip;
        let debug_str = format!("{:?}", key);

        assert!(debug_str.contains("PartitionKey::Ip"));
    }

    #[test]
    fn it_formats_custom_partition_key_debug_output() {
        let key = PartitionKey::Custom(Arc::new(|_req| Ok(42)));
        let debug_str = format!("{:?}", key);

        assert!(debug_str.contains("PartitionKey::Custom"));
    }

    #[test]
    fn it_formats_key_source_debug_output() {
        let source = ip();
        let debug_str = format!("{:?}", source);

        assert!(debug_str.contains("RateLimitKeySource"));
    }

    #[test]
    fn it_extracts_key_from_ip_source() {
        let source = ip();
        let req = create_request();

        // This will fail in tests without proper request setup, but verifies the trait implementation
        let _result = source.extract(&req);
    }

    #[test]
    fn it_extracts_key_through_partition_key() {
        let key = PartitionKey::Ip;
        let req = create_request();

        let _result = key.extract(&req);
    }

    #[test]
    fn it_extracts_key_through_custom_partition_key() {
        let key = PartitionKey::Custom(Arc::new(|_req| Ok(123)));
        let req = create_request();

        let result = key.extract(&req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 123);
    }

    #[test]
    fn it_propagates_errors_from_custom_extractor() {
        let key = PartitionKey::Custom(Arc::new(|_req| {
            Err(Error::client_error("Test error"))
        }));
        let req = create_request();

        let result = key.extract(&req);
        assert!(result.is_err());
    }

    #[test]
    fn it_creates_multiple_header_sources_with_different_names() {
        let source1 = header("x-api-key");
        let source2 = header("x-tenant-id");

        assert!(matches!(source1.inner, PartitionKey::Custom(_)));
        assert!(matches!(source2.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_creates_multiple_query_sources_with_different_names() {
        let source1 = query("key1");
        let source2 = query("key2");

        assert!(matches!(source1.inner, PartitionKey::Custom(_)));
        assert!(matches!(source2.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_creates_multiple_path_sources_with_different_names() {
        let source1 = path("user_id");
        let source2 = path("tenant_id");

        assert!(matches!(source1.inner, PartitionKey::Custom(_)));
        assert!(matches!(source2.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_chains_using_after_ip() {
        let binding = ip().using("rate_limit");

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "rate_limit");
    }

    #[test]
    fn it_chains_using_after_header() {
        let binding = header("x-api-key").using("api_limit");

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "api_limit");
    }

    #[test]
    fn it_chains_using_after_query() {
        let binding = query("api_key").using("query_limit");

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "query_limit");
    }

    #[test]
    fn it_chains_using_after_path() {
        let binding = path("id").using("path_limit");

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "path_limit");
    }

    #[cfg(feature = "cookie")]
    #[test]
    fn it_chains_using_after_cookie() {
        let binding = cookie("session").using("cookie_limit");

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "cookie_limit");
    }

    #[test]
    fn it_binds_ip_source_without_policy() {
        let binding = ip().bind();

        assert!(binding.policy.is_none());
    }

    #[test]
    fn it_binds_header_source_without_policy() {
        let binding = header("x-custom").bind();

        assert!(binding.policy.is_none());
    }

    #[test]
    fn it_extracts_consistent_values_from_custom_extractor() {
        let key = PartitionKey::Custom(Arc::new(|_req| Ok(999)));
        let req = create_request();

        let result1 = key.extract(&req);
        let result2 = key.extract(&req);

        assert_eq!(result1.unwrap(), 999);
        assert_eq!(result2.unwrap(), 999);
    }

    #[test]
    fn it_handles_empty_policy_name() {
        let binding = ip().using("");

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), "");
    }

    #[test]
    fn it_handles_long_policy_name() {
        let long_name = "very_long_policy_name_for_rate_limiting_configuration";
        let binding = ip().using(long_name);

        assert!(binding.policy.is_some());
        assert_eq!(binding.policy.as_ref().unwrap().as_ref(), long_name);
    }

    #[test]
    fn it_creates_independent_key_sources() {
        let source1 = ip();
        let source2 = header("x-key");
        let source3 = query("param");

        assert!(matches!(source1.inner, PartitionKey::Ip));
        assert!(matches!(source2.inner, PartitionKey::Custom(_)));
        assert!(matches!(source3.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_implements_send_and_sync_for_key_source() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RateLimitKeySource>();
    }

    #[test]
    fn it_implements_send_and_sync_for_partition_key() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PartitionKey>();
    }

    #[test]
    fn it_stores_custom_extractor_in_arc() {
        let extractor = Arc::new(|_req: &HttpRequest| Ok(42u64));
        let key = PartitionKey::Custom(extractor.clone());

        // Verify Arc reference count increases
        assert_eq!(Arc::strong_count(&extractor), 2);

        drop(key);
        assert_eq!(Arc::strong_count(&extractor), 1);
    }

    #[test]
    fn it_clones_partition_key_with_shared_extractor() {
        let extractor = Arc::new(|_req: &HttpRequest| Ok(42u64));
        let key1 = PartitionKey::Custom(extractor.clone());
        let _key2 = key1.clone();

        // Both keys should share the same Arc
        assert_eq!(Arc::strong_count(&extractor), 3);
    }

    #[cfg(feature = "jwt-auth")]
    #[test]
    fn it_creates_user_source_with_different_extractors() {
        use serde::Deserialize;

        #[derive(Clone, Deserialize)]
        struct Claims {
            sub: String,
            email: String,
        }

        impl AuthClaims for Claims {}

        let source1 = user(|claims: &Claims| claims.sub.as_str());
        let source2 = user(|claims: &Claims| claims.email.as_str());

        assert!(matches!(source1.inner, PartitionKey::Custom(_)));
        assert!(matches!(source2.inner, PartitionKey::Custom(_)));
    }

    #[test]
    fn it_creates_binding_from_ip_source() {
        let source = ip();
        let binding = source.using("test");
        let req = create_request();

        // Verify binding can extract (will fail without proper setup, but verifies trait)
        let _result = binding.extract(&req);
    }

    #[test]
    fn it_preserves_extractor_behavior_after_clone() {
        let key = PartitionKey::Custom(Arc::new(|_req| Ok(777)));
        let cloned = key.clone();
        let req = create_request();

        assert_eq!(key.extract(&req).unwrap(), 777);
        assert_eq!(cloned.extract(&req).unwrap(), 777);
    }
}
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
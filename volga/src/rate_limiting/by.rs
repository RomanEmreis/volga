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
//! // Rate limit by authenticated user
//! by::user(|claims| claims.sub.as_str());
//! ```

use std::sync::Arc;
use super::{extract_partition_key_from_ip, stable_hash, RateLimitKey};
use crate::{HttpRequest, headers::{HeaderName, HeaderError}, error::Error};

#[cfg(feature = "cookie")]
use crate::http::Cookies;

#[cfg(feature = "jwt-auth")]
use crate::auth::{AuthClaims, Authenticated};

/// A function that extracts a rate limiting partition key from an HTTP request.
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

impl RateLimitKey for PartitionKey {
    #[inline]
    fn extract(&self, req: &HttpRequest) -> Result<u64, Error> {
        match self {
            PartitionKey::Ip => extract_partition_key_from_ip(req),
            PartitionKey::Custom(extractor) => extractor(req)
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
pub fn ip() -> impl RateLimitKey {
    PartitionKey::Ip
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
pub fn header(name: &'static str) -> impl RateLimitKey {
    let header = HeaderName::from_static(name);

    PartitionKey::Custom(Arc::new(move |req| {
        let value = req.headers()
            .get(&header)
            .ok_or_else(|| HeaderError::header_missing_impl(name))?;

        let value = value.to_str()
            .map_err(HeaderError::from_to_str_error)?;

        Ok(stable_hash(value))
    }))
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
pub fn query(name: &'static str) -> impl RateLimitKey {
    PartitionKey::Custom(Arc::new(move |req| {
        let value = req.query_args()
            .find_map(|(k, v)| if k == name { Some(v) } else { None })
            .ok_or_else(|| Error::client_error(format!("Query parameter {name} not found", )))?;

        Ok(stable_hash(value))
    }))
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
pub fn path(name: &'static str) -> impl RateLimitKey {
    PartitionKey::Custom(Arc::new(move |req| {
        let value = req.path_args()
            .find_map(|(k, v)| if k == name { Some(v) } else { None })
            .ok_or_else(|| Error::client_error(format!("Path parameter {name} not found", )))?;

        Ok(stable_hash(value))
    }))
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
pub fn cookie(name: &'static str) -> impl RateLimitKey {
    PartitionKey::Custom(Arc::new(move |req| {
        let cookies = req.extract::<Cookies>()?;
        let cookie = cookies.get(name)
            .ok_or_else(|| Error::client_error(format!("Cookie {name} not found", )))?;

        Ok(stable_hash(cookie.value()))
    }))
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
/// use volga::{App, auth::AuthClaims rate_limiting::by};
///
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
/// app.use_fixed_window(by::user(|claims| claims.sub.as_str()));
///
/// // Rate limit per email
/// app.use_fixed_window(by::user(|claims| claims.email.as_str()));
/// ```
///
/// # Notes
/// - This function requires the `jwt-auth` feature.
/// - If authentication has not been performed for the request,
///   key extraction will fail.
#[inline]
#[cfg(feature = "jwt-auth")]
pub fn user<C, F>(f: F) -> impl RateLimitKey
where
    C: AuthClaims + Send + Sync + 'static,
    F: Fn(&C) -> &str + Send + Sync + 'static
{
    PartitionKey::Custom(Arc::new(move |req| {
        let auth = req.extract::<Authenticated<C>>()?;
        let key = f(auth.claims());
        Ok(stable_hash(key))
    }))
}
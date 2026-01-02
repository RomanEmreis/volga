//! Tools and utilities for Rate Limiting

use twox_hash::XxHash64;
use smallvec::SmallVec;
use std::{
    hash::{Hash, Hasher}, 
    net::{IpAddr, SocketAddr}, 
};
use std::fmt::Debug;
use crate::{
    App,
    ClientIp,
    HttpRequest,
    HttpResult,
    routing::{Route, RouteGroup},
    middleware::{HttpContext, NextFn},
    http::StatusCode,
    headers::FORWARDED,
    error::Error,
    status
};

pub use fixed_window::FixedWindow;
pub use sliding_window::SlidingWindow;
pub use key::{RateLimitKey, RateLimitKeyExt, PolicyName, RateLimitBinding};
pub use by::RateLimitKeySource;

pub use volga_rate_limiter::{
    FixedWindowRateLimiter,
    SlidingWindowRateLimiter,
    RateLimiter
};

mod fixed_window;
mod sliding_window;
mod key;
pub mod by;

const X_FORWARDED_FOR: &str = "x-forwarded-for";
const DEFAULT_POLICIES_COUNT: usize = 4;
const RATE_LIMIT_ERROR_MSG: &str = "Rate limit exceeded. Try again later.";

/// A tuple representing a named policy entry: `(policy_name, limiter)`.
///
/// Used internally in `GlobalRateLimiter` to store named rate-limiting policies.
type PolicyEntry<T> = (String, T);

/// Represents the global rate limiting configuration for the application.
///
/// This structure holds both **fixed window** and **sliding window** limiters,
/// each of which can have a **default** limiter (applied when no policy name is specified)
/// and **named policies** (applied when a specific policy name is used).
///
/// Typically, this structure is managed internally by the `App` and should not be
/// accessed directly from middleware; instead, helper methods provide access to
/// the appropriate limiter by policy name.
#[derive(Debug, Default)]
pub(crate) struct GlobalRateLimiter {
    /// Default fixed window rate limiter (used when no policy name is specified)
    default_fixed_window: Option<FixedWindowRateLimiter>,
    /// Named fixed window rate limiters
    named_fixed_window: SmallVec<[PolicyEntry<FixedWindowRateLimiter>; DEFAULT_POLICIES_COUNT]>,

    /// Default sliding window rate limiter (used when no policy name is specified)
    default_sliding_window: Option<SlidingWindowRateLimiter>,
    /// Named sliding window rate limiters
    named_sliding_window: SmallVec<[PolicyEntry<SlidingWindowRateLimiter>; DEFAULT_POLICIES_COUNT]>,
}

impl GlobalRateLimiter {
    /// Adds a fixed window rate limiting policy to the global configuration.
    ///
    /// - If the policy has a `name`, it will be stored in `named_fixed_window`.
    /// - If the policy has no `name`, it will become the `default_fixed_window`.
    #[inline]
    fn add_fixed_window(&mut self, policy: FixedWindow) {
        let limiter = policy.build();
        let name = policy.name;
        match name {
            None => self.default_fixed_window = Some(limiter),
            Some(name) => self.named_fixed_window.push((name, limiter))
        }
    }

    /// Adds a sliding window rate limiting policy to the global configuration.
    ///
    /// - If the policy has a `name`, it will be stored in `named_sliding_window`.
    /// - If the policy has no `name`, it will become the `default_sliding_window`.
    #[inline]
    fn add_sliding_window(&mut self, policy: SlidingWindow) {
        let limiter = policy.build();
        let name = policy.name;
        match name {
            None => self.default_sliding_window = Some(limiter),
            Some(name) => self.named_sliding_window.push((name, limiter))
        }
    }

    /// Returns a reference to a fixed window rate limiter by policy name.
    ///
    /// - `policy_name = None` -> returns the default fixed window limiter.
    /// - `policy_name = Some(name)` -> returns the named fixed window limiter if it exists.
    #[inline]
    pub(crate) fn fixed_window(&self, policy_name: Option<&str>) -> Option<&FixedWindowRateLimiter> {
        match policy_name {
            None => self.default_fixed_window.as_ref(),
            Some(name) => self.named_fixed_window.iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v),
        }
    }

    /// Returns a reference to a sliding window rate limiter by policy name.
    ///
    /// - `policy_name = None` -> returns the default sliding window limiter.
    /// - `policy_name = Some(name)` -> returns the named sliding window limiter if it exists.
    #[inline]
    pub(crate) fn sliding_window(&self, policy_name: Option<&str>) -> Option<&SlidingWindowRateLimiter> {
        match policy_name {
            None => self.default_sliding_window.as_ref(),
            Some(name) => self.named_sliding_window.iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v),
        }
    }
}

impl App {
    /// Registers a fixed-window rate limiting policy.
    ///
    /// This method defines **how** rate limiting should work (limits, window size,
    /// eviction behavior), but does **not** enable rate limiting by itself.
    /// To actually apply the policy to incoming requests, it must be referenced
    /// from rate-limiting middleware (see [`App::use_fixed_window`] or [`Route::fixed_window`]).
    ///
    /// ## Named vs. default policy
    ///
    /// - If the policy has a name (`FixedWindow::with_name`), it is registered
    ///   as a **named policy** and can be referenced explicitly by name.
    /// - If no name is provided, the policy becomes the **default fixed-window
    ///   policy**, used when no policy name is specified in middleware.
    ///
    /// Registering a policy with the same name multiple times will override
    /// the previously registered policy.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use volga::{
    ///     App,
    ///     rate_limiting::{by, FixedWindow, RateLimitKeyExt},
    /// };
    ///
    /// # fn main() {
    /// // Define a fixed window rate-limiting policy
    /// let fixed_window = FixedWindow::new(100, Duration::from_secs(30))
    ///     .with_name("burst");
    ///
    /// // Register the policy in the application
    /// let mut app = App::new()
    ///     .with_fixed_window(fixed_window);
    ///
    /// // Enable fixed-window rate limiting using the "burst" policy,
    /// // partitioned by the client IP address
    /// app.use_fixed_window(by::ip().using("burst"));
    ///
    /// # app.run_blocking();
    /// # }
    /// ```
    ///
    /// ## See also
    ///
    /// - [`FixedWindow`] - fixed window policy definition
    /// - [`App::use_fixed_window`] - enabling fixed-window rate limiting middleware
    /// - [`Route::fixed_window`] - enabling fixed-window rate limiting middleware for a particular route
    /// - [`RouteGroup::fixed_window`] - enabling fixed-window rate limiting middleware for a route group
    pub fn with_fixed_window(mut self, policy: FixedWindow) -> Self {
        self.rate_limiter
            .get_or_insert_default()
            .add_fixed_window(policy);
        self
    }

    /// Registers a sliding-window rate limiting policy.
    ///
    /// This method defines **how** rate limiting should work (limits, window size,
    /// eviction behavior), but does **not** enable rate limiting by itself.
    /// To actually apply the policy to incoming requests, it must be referenced
    /// from rate-limiting middleware (see [`App::use_sliding_window`] or [`Route::sliding_window`]).
    ///
    /// ## Named vs. default policy
    ///
    /// - If the policy has a name (`SlidingWindow::with_name`), it is registered
    ///   as a **named policy** and can be referenced explicitly by name.
    /// - If no name is provided, the policy becomes the **default sliding-window
    ///   policy**, used when no policy name is specified in middleware.
    ///
    /// Registering a policy with the same name multiple times will override
    /// the previously registered policy.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use volga::{
    ///     App,
    ///     rate_limiting::{by, SlidingWindow, RateLimitKeyExt},
    /// };
    ///
    /// # fn main() {
    /// // Define a sliding window rate-limiting policy
    /// let sliding_window = SlidingWindow::new(100, Duration::from_secs(30))
    ///     .with_name("burst");
    ///
    /// // Register the policy in the application
    /// let mut app = App::new()
    ///     .with_sliding_window(sliding_window);
    ///
    /// // Enable sliding-window rate limiting using the "burst" policy,
    /// // partitioned by the client IP address
    /// app.use_sliding_window(by::ip().using("burst"));
    ///
    /// # app.run_blocking();
    /// # }
    /// ```
    ///
    /// ## See also
    ///
    /// - [`SlidingWindow`] - fixed window policy definition
    /// - [`App::use_sliding_window`] - enabling fixed-window rate limiting middleware
    /// - [`Route::sliding_window`] - enabling fixed-window rate limiting middleware for a particular route
    /// - [`RouteGroup::sliding_window`] - enabling fixed-window rate limiting middleware for a route group
    pub fn with_sliding_window(mut self, policy: SlidingWindow) -> Self {
        self.rate_limiter
            .get_or_insert_default()
            .add_sliding_window(policy);
        self
    }

    /// Enables fixed-window rate limiting for incoming requests.
    ///
    /// This method installs a **global middleware** that applies a fixed-window
    /// rate limiter to all requests passing through the application.
    ///
    /// The provided `source` defines:
    /// - **How requests are partitioned** (e.g. by IP, user, header, path)
    /// - **Which rate-limiting policy is used** (default or named)
    ///
    /// The middleware will look up a previously registered [`FixedWindow`]
    /// policy and apply it to each request. If no matching policy is found,
    /// the middleware is a no-op.
    ///
    /// ## Partition keys
    ///
    /// The partition key determines how requests are grouped for rate limiting.
    /// Common examples include:
    /// - Client IP address
    /// - Authenticated user ID
    /// - API key or header value
    /// - Route or tenant identifier
    ///
    /// ## Policy selection
    ///
    /// - If the key is bound **without** a policy name, the **default fixed-window
    ///   policy** is used.
    /// - If the key is bound **with** `.using(name)`, the named policy with the
    ///   corresponding name is applied.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use volga::{
    ///     App,
    ///     rate_limiting::{by, FixedWindow, RateLimitKeyExt},
    /// };
    ///
    /// # fn main() {
    /// let mut app = App::new()
    ///     // Register a default fixed-window policy
    ///     .with_fixed_window(
    ///         FixedWindow::new(60, Duration::from_secs(60))
    ///     )
    ///     // Register a named policy for burst traffic
    ///     .with_fixed_window(
    ///         FixedWindow::new(100, Duration::from_secs(30))
    ///             .with_name("burst")
    ///     );
    ///
    /// // Apply rate limiting by client IP using the default policy
    /// app.use_fixed_window(by::ip());
    ///
    /// // Apply rate limiting by user ID using the "burst" policy
    /// app.use_fixed_window(by::header("x-tenant-id").using("burst"));
    ///
    /// # app.run_blocking()
    /// # }
    /// ```
    ///
    /// ## Notes
    ///
    /// - This middleware is **global** and affects all routes registered
    ///   after it is applied.
    /// - Multiple rate-limiting middlewares may be installed, each with
    ///   its own partition key and policy.
    /// - Rate limiting failures result in an HTTP `429 Too Many Requests` response.
    ///
    /// ## See also
    ///
    /// - [`FixedWindow`] — fixed window policy definition
    /// - [`App::with_fixed_window`] — registering fixed-window policies
    /// - [`RateLimitKeyExt`] — binding partition keys to policies
    pub fn use_fixed_window<K: RateLimitKeyExt>(&mut self, source: K) -> &mut Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_fixed_window(ctx, binding.clone(), next))
    }

    /// Enables sliding-window rate limiting for incoming requests.
    ///
    /// This method installs a **global middleware** that applies a sliding-window
    /// rate limiter to all requests passing through the application.
    ///
    /// The provided `source` defines:
    /// - **How requests are partitioned** (e.g. by IP, user, header, path)
    /// - **Which rate-limiting policy is used** (default or named)
    ///
    /// The middleware will look up a previously registered [`SlidingWindow`]
    /// policy and apply it to each request. If no matching policy is found,
    /// the middleware is a no-op.
    ///
    /// ## Partition keys
    ///
    /// The partition key determines how requests are grouped for rate limiting.
    /// Common examples include:
    /// - Client IP address
    /// - Authenticated user ID
    /// - API key or header value
    /// - Route or tenant identifier
    ///
    /// ## Policy selection
    ///
    /// - If the key is bound **without** a policy name, the **default sliding-window
    ///   policy** is used.
    /// - If the key is bound **with** `.using(name)`, the named policy with the
    ///   corresponding name is applied.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use volga::{
    ///     App,
    ///     rate_limiting::{by, SlidingWindow, RateLimitKeyExt},
    /// };
    ///
    /// # fn main() {
    /// let mut app = App::new()
    ///     // Register a default sliding-window policy
    ///     .with_sliding_window(
    ///         SlidingWindow::new(60, Duration::from_secs(60))
    ///     )
    ///     // Register a named policy for burst traffic
    ///     .with_sliding_window(
    ///         SlidingWindow::new(100, Duration::from_secs(30))
    ///             .with_name("burst")
    ///     );
    ///
    /// // Apply rate limiting by client IP using the default policy
    /// app.use_sliding_window(by::ip());
    ///
    /// // Apply rate limiting by user ID using the "burst" policy
    /// app.use_sliding_window(by::header("x-tenant-id").using("burst"));
    ///
    /// # app.run_blocking()
    /// # }
    /// ```
    ///
    /// ## Notes
    ///
    /// - This middleware is **global** and affects all routes registered
    ///   after it is applied.
    /// - Multiple rate-limiting middlewares may be installed, each with
    ///   its own partition key and policy.
    /// - Rate limiting failures result in an HTTP `429 Too Many Requests` response.
    ///
    /// ## See also
    ///
    /// - [`SlidingWindow`] — fixed window policy definition
    /// - [`App::with_sliding_window`] — registering fixed-window policies
    /// - [`RateLimitKeyExt`] — binding partition keys to policies
    pub fn use_sliding_window<K: RateLimitKeyExt>(&mut self, source: K) -> &mut Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_sliding_window(ctx, binding.clone(), next))
    }
}

impl<'a> Route<'a> {
    /// Enables fixed-window rate limiting for this route.
    ///
    /// This method installs a **route-scoped middleware** that applies a
    /// fixed-window rate limiter **only** to requests handled by this route.
    ///
    /// The provided `source` defines:
    /// - How requests are partitioned (e.g. by IP, user, header, path)
    /// - Which fixed-window policy is used (default or named)
    ///
    /// ## Policy resolution
    ///
    /// - If the partition key is bound **without** a policy name, the
    ///   default fixed-window policy is used.
    /// - If `using(name)` is specified, the named policy with the
    ///   corresponding name is applied.
    ///
    /// ## Middleware order
    ///
    /// Route-level rate limiting is executed:
    /// - **after global middleware**
    /// - **after route group middleware**
    /// - **before the route handler**
    ///
    /// This allows route-specific limits to refine or override
    /// broader rate-limiting rules.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use volga::rate_limiting::{by, RateLimitKeyExt};
    ///
    /// # let mut app = volga::App::new();
    /// app.map_get("/api/private", || async { /*...*/ })
    ///     .fixed_window(by::ip().using("burst"));
    /// ```
    ///
    /// ## Notes
    ///
    /// - Multiple rate-limiting middlewares may be attached to the same route.
    /// - A rate limit violation results in an HTTP `429 Too Many Requests` response.
    pub fn fixed_window<K: RateLimitKeyExt>(self, source: K) -> Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_fixed_window(ctx, binding.clone(), next))
    }

    /// Enables sliding-window rate limiting for this route.
    ///
    /// This method installs a **route-scoped middleware** that applies a
    /// sliding-window rate limiter **only** to requests handled by this route.
    ///
    /// The provided `source` defines:
    /// - How requests are partitioned (e.g. by IP, user, header, path)
    /// - Which sliding-window policy is used (default or named)
    ///
    /// ## Policy resolution
    ///
    /// - If the partition key is bound **without** a policy name, the
    ///   default sliding-window policy is used.
    /// - If `using(name)` is specified, the named policy with the
    ///   corresponding name is applied.
    ///
    /// ## Middleware order
    ///
    /// Route-level rate limiting is executed:
    /// - **after global middleware**
    /// - **after route group middleware**
    /// - **before the route handler**
    ///
    /// This allows route-specific limits to refine or override
    /// broader rate-limiting rules.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use volga::rate_limiting::{by, RateLimitKeyExt};
    ///
    /// # let mut app = volga::App::new();
    /// app.map_get("/api/private", || async { /*...*/ })
    ///     .sliding_window(by::ip().using("burst"));
    /// ```
    ///
    /// ## Notes
    ///
    /// - Multiple rate-limiting middlewares may be attached to the same route.
    /// - A rate limit violation results in an HTTP `429 Too Many Requests` response.
    pub fn sliding_window<K: RateLimitKeyExt>(self, source: K) -> Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_sliding_window(ctx, binding.clone(), next))
    }
}

impl<'a> RouteGroup<'a> {
    /// Enables fixed-window rate limiting for all routes in this group.
    ///
    /// This method installs a **group-scoped middleware** that applies a
    /// fixed-window rate limiter to every route contained within the group.
    ///
    /// Group-level rate limiting allows sharing a common rate-limiting
    /// strategy across multiple related routes.
    ///
    /// ## Policy resolution
    ///
    /// - Uses the default fixed-window policy unless overridden with `using(name)`.
    /// - Named policies must be registered via [`App::with_fixed_window`].
    ///
    /// ## Middleware order
    ///
    /// Group-level rate limiting is executed:
    /// - **after global middleware**
    /// - **before route-level middleware**
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use volga::rate_limiting::{by, RateLimitKeyExt};
    ///
    /// # let mut app = volga::App::new();
    /// app.group("/api", |api| {
    ///     api.fixed_window(by::ip());
    /// 
    ///     api.map_get("/status", || async { /*...*/ });
    ///     api.map_post("/upload", || async { /*...*/ })
    ///         .fixed_window(by::header("x-tenant-id").using("burst"));
    /// });
    /// ```
    pub fn fixed_window<K: RateLimitKeyExt>(&mut self, source: K) -> &mut Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_fixed_window(ctx, binding.clone(), next))
    }

    /// Enables sliding-window rate limiting for all routes in this group.
    ///
    /// This method installs a **group-scoped middleware** that applies a
    /// sliding-window rate limiter to every route contained within the group.
    ///
    /// Group-level rate limiting allows sharing a common rate-limiting
    /// strategy across multiple related routes.
    ///
    /// ## Policy resolution
    ///
    /// - Uses the default fixed-window policy unless overridden with `using(name)`.
    /// - Named policies must be registered via [`App::with_sliding_window`].
    ///
    /// ## Middleware order
    ///
    /// Group-level rate limiting is executed:
    /// - **after global middleware**
    /// - **before route-level middleware**
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use volga::rate_limiting::{by, RateLimitKeyExt};
    ///
    /// # let mut app = volga::App::new();
    /// app.group("/api", |api| {
    ///     api.sliding_window(by::ip());
    ///
    ///     api.map_get("/status", || async { /*...*/ });
    ///     api.map_post("/upload", || async { /*...*/ })
    ///         .sliding_window(by::header("x-tenant-id").using("burst"));
    /// });
    /// ```
    pub fn sliding_window<K: RateLimitKeyExt>(&mut self, source: K) -> &mut Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_sliding_window(ctx, binding.clone(), next))
    }
}

#[inline]
async fn check_fixed_window(
    ctx: HttpContext,
    binding: RateLimitBinding, 
    next: NextFn
) -> HttpResult {
    if let Some(limiter) = ctx.fixed_window_rate_limiter(binding.policy.as_deref()) {
        let key = binding.key.extract(&ctx.request)?;
        if !limiter.check(key) { 
            status!(
                StatusCode::TOO_MANY_REQUESTS.as_u16(), 
                RATE_LIMIT_ERROR_MSG
            )
        } else {
            next(ctx).await
        }
    } else { 
        next(ctx).await
    }
}

#[inline]
async fn check_sliding_window(
    ctx: HttpContext, 
    binding: RateLimitBinding, 
    next: NextFn
) -> HttpResult {
    if let Some(limiter) = ctx.sliding_window_rate_limiter(binding.policy.as_deref()) {
        let key = binding.key.extract(&ctx.request)?;
        if !limiter.check(key) { 
            status!(
                StatusCode::TOO_MANY_REQUESTS.as_u16(), 
                RATE_LIMIT_ERROR_MSG
            )
        } else {
            next(ctx).await
        }
    } else { 
        next(ctx).await
    }
}

#[inline]
fn extract_partition_key_from_ip(req: &HttpRequest) -> Result<u64, Error> {
    let ip = req.extract::<ClientIp>()?;
    let client_ip = extract_client_ip(req, ip.into_inner());
    Ok(stable_hash(&client_ip))
}

#[inline]
fn stable_hash<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    value.hash(&mut hasher);
    hasher.finish()
}

fn extract_client_ip(req: &HttpRequest, remote_addr: SocketAddr) -> IpAddr {
    // RFC 7239 Forwarded
    if let Some(ip) = forwarded_header(req) {
        return ip;
    }

    // X-Forwarded-For
    if let Some(ip) = x_forwarded_for(req) {
        return ip;
    }

    // Fallback
    remote_addr.ip()
}

#[inline]
fn forwarded_header(req: &HttpRequest) -> Option<IpAddr> {
    let header = req.headers().get(FORWARDED)?.to_str().ok()?;
    header.split(';')
        .find_map(|part| {
            let part = part.trim();
            part.strip_prefix("for=")
        })
        .and_then(|v| {
            let v = v.trim_matches('"');
            v.parse::<IpAddr>().ok()
        })
}

#[inline]
fn x_forwarded_for(req: &HttpRequest) -> Option<IpAddr> {
    let header = req.headers().get(X_FORWARDED_FOR)?.to_str().ok()?;
    header.split(',')
        .next()
        .map(str::trim)
        .and_then(|ip| ip.parse::<IpAddr>().ok())
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use hyper::http::HeaderName;
    use hyper::Request;
    use crate::HttpBody;
    use super::*;
    
    fn create_request() -> HttpRequest {
        let (parts, body) = Request::get("/")
            .extension(ClientIp(SocketAddr::new(IpAddr::V4(127_u32.into()), 8080)))
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        
        HttpRequest::from_parts(parts, body)
    }
    
    #[test]
    fn it_extracts_partition_key_from_ip() {
        let req = create_request();
        
        let key = extract_partition_key_from_ip(&req).unwrap();
        
        assert_eq!(key, stable_hash(&IpAddr::V4(127_u32.into())));
    }
    
    #[test]
    fn it_extracts_forwarded_ip() {
        let mut req = create_request();
        req.inner
            .headers_mut()
            .insert(HeaderName::from_static("forwarded"), "for=192.168.1.1".parse().unwrap());
        
        let key = extract_partition_key_from_ip(&req).unwrap();
        
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_extracts_x_forwarded_for_ip() {
        let mut req = create_request();
        req.inner
            .headers_mut()
            .insert(HeaderName::from_static("x-forwarded-for"), "192.168.1.1".parse().unwrap());

        let key = extract_partition_key_from_ip(&req).unwrap();

        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_extracts_prioritized_forwarded_ip() {
        let mut req = create_request();
        req.inner
            .headers_mut()
            .insert(HeaderName::from_static("forwarded"), "for=10.24.1.101".parse().unwrap());
        req.inner
            .headers_mut()
            .insert(HeaderName::from_static("x-forwarded-for"), "192.168.1.1".parse().unwrap());

        let key = extract_partition_key_from_ip(&req).unwrap();

        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(10, 24, 1, 101))));
    }
    
    #[test]
    fn it_tests_stable_hash() {
        let key = stable_hash(&IpAddr::V4(127_u32.into()));
        assert_eq!(key, stable_hash(&IpAddr::V4(127_u32.into())));
    }
}

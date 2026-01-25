//! Tools and utilities for Rate Limiting

use twox_hash::XxHash64;
use smallvec::SmallVec;
use std::{
    collections::HashSet,
    hash::{Hash, Hasher}, 
    net::{IpAddr, SocketAddr},
    sync::Arc,
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
const MAX_FORWARDED_IPS: usize = 16;
const MAX_FORWARDED_HEADER_LEN: usize = 2 * 1024;
const DEFAULT_POLICIES_COUNT: usize = 4;
const DEFAULT_IPS_COUNT: usize = 4;
const RATE_LIMIT_ERROR_MSG: &str = "Rate limit exceeded. Try again later.";

/// Represents trusted proxies for rate limiting IP extraction
#[derive(Clone, Debug)]
pub(crate) struct TrustedProxies(pub(crate) Arc<HashSet<IpAddr>>);

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

    /// Defines a list of trusted proxies used when extracting client IPs
    /// for rate limiting. Forwarded headers are honored only if the
    /// incoming connection is from one of these proxies.
    ///
    /// If an empty list is provided, forwarded headers will be ignored.
    pub fn with_trusted_proxies<I, T>(mut self, proxies: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<IpAddr>
    {
        let proxies: HashSet<IpAddr> = proxies.into_iter().map(Into::into).collect();
        self.trusted_proxies = if proxies.is_empty() {
            None
        } else {
            Some(proxies)
        };
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
        let key = binding.key.extract(ctx.request())?;
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
        let key = binding.key.extract(ctx.request())?;
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
    let peer = remote_addr.ip();

    let Some(trusted) = req.extensions().get::<TrustedProxies>() else {
        return peer;
    };

    // Don't trust headers unless the direct peer is trusted
    if !trusted.0.contains(&peer) {
        return peer;
    }

    let chain = forwarded_chain(req)
        .or_else(|| x_forwarded_for_chain(req));

    let Some(mut chain) = chain else {
        return peer;
    };

    if chain.last().copied() != Some(peer) {
        chain.push(peer);
    }

    for ip in chain.iter() {
        if !trusted.0.contains(ip) {
            return *ip;
        }
    }

    // all hops trusted - best guess: left-most (original) or peer
    chain.last().copied().unwrap_or(peer)
}

#[inline]
fn forwarded_chain(req: &HttpRequest) -> Option<SmallVec<[IpAddr; DEFAULT_IPS_COUNT]>> {
    let header = req.headers().get(FORWARDED)?.to_str().ok()?;
    if header.len() > MAX_FORWARDED_HEADER_LEN {
        return None;
    }
    
    let mut out = SmallVec::new();

    for entry in header.rsplit(',').take(MAX_FORWARDED_IPS) {
        // entry: for=...;proto=...;by=...
        for part in entry.split(';') {
            let part = part.trim();
            let Some(v) = part.strip_prefix("for=") else { 
                continue
            };

            let v = v.trim().trim_matches('"'); // remove quotes

            // RFC allows: for=unknown or obfuscated identifiers; ignore those
            if v.eq_ignore_ascii_case("unknown") || v.starts_with('_') {
                continue;
            }

            // Handle bracketed IPv6, optionally with port: [v6] or [v6]:port
            if let Some(rest) = v.strip_prefix('[') {
                if let Some((inside, _port)) = rest.split_once(']') {
                    // after is "" or ":port" (or garbage). We ignore port.
                    if let Ok(ip) = inside.parse::<IpAddr>() {
                        out.push(ip);
                    }
                }
                continue;
            }

            // Remove port if present:
            // - IPv4: 1.2.3.4:123
            // - IPv6 might come as 2001:db8::1 (no port) OR [2001:db8::1]:123 
            // (brackets already handled above, so the port form should have been bracketed; but be defensive)
            let ip_str = if let Some((host, _port)) = v.rsplit_once(':') {
                // Heuristic: only treat as host:port if host parses as IpAddr
                if host.parse::<IpAddr>().is_ok() { 
                    host
                } else {
                    v
                }
            } else {
                v
            };

            if let Ok(ip) = ip_str.parse::<IpAddr>() {
                out.push(ip);
            }
        }
    }

    (!out.is_empty()).then_some(out)
}

#[inline]
fn x_forwarded_for_chain(req: &HttpRequest) -> Option<SmallVec<[IpAddr; DEFAULT_IPS_COUNT]>> {
    let header = req.headers().get(X_FORWARDED_FOR)?.to_str().ok()?;
    if header.len() > MAX_FORWARDED_HEADER_LEN {
        return None;
    }
    
    let mut out = SmallVec::new();

    for part in header.rsplit(',').take(MAX_FORWARDED_IPS) {
        let s = part.trim();
        if s.is_empty() { 
            continue;
        }
        
        if let Ok(ip) = s.parse::<IpAddr>() {
            out.push(ip);
        }
    }

    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::net::Ipv4Addr;
    use std::time::Duration;
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

    fn create_request_with_trusted_proxy() -> HttpRequest {
        create_request_with_specific_trusted_proxy_chain(
            IpAddr::V4(127_u32.into()), 
            [IpAddr::V4(127_u32.into())]
        )
    }

    fn create_request_with_specific_trusted_proxy_chain(
        peer: IpAddr,
        trusted_proxies: impl IntoIterator<Item = IpAddr>
    ) -> HttpRequest {
        let (mut parts, body) = Request::get("/")
            .extension(ClientIp(SocketAddr::new(peer, 8080)))
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let trusted = trusted_proxies.into_iter().collect();
        parts.extensions.insert(TrustedProxies(Arc::new(trusted)));

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
        let mut req = create_request_with_trusted_proxy();
        req
            .headers_mut()
            .insert(HeaderName::from_static("forwarded"), "for=192.168.1.1".parse().unwrap());
        
        let key = extract_partition_key_from_ip(&req).unwrap();
        
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_extracts_x_forwarded_for_ip() {
        let mut req = create_request_with_trusted_proxy();
        req
            .headers_mut()
            .insert(HeaderName::from_static("x-forwarded-for"), "192.168.1.1".parse().unwrap());

        let key = extract_partition_key_from_ip(&req).unwrap();

        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_extracts_prioritized_forwarded_ip() {
        let mut req = create_request_with_trusted_proxy();
        req
            .headers_mut()
            .insert(HeaderName::from_static("forwarded"), "for=10.24.1.101".parse().unwrap());
        req
            .headers_mut()
            .insert(HeaderName::from_static("x-forwarded-for"), "192.168.1.1".parse().unwrap());

        let key = extract_partition_key_from_ip(&req).unwrap();

        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(10, 24, 1, 101))));
    }

    #[test]
    fn it_ignores_forwarded_when_proxy_is_untrusted() {
        let mut req = create_request();
        req
            .headers_mut()
            .insert(HeaderName::from_static("forwarded"), "for=192.168.1.1".parse().unwrap());

        let key = extract_partition_key_from_ip(&req).unwrap();

        assert_eq!(key, stable_hash(&IpAddr::V4(127_u32.into())));
    }

    #[test]
    fn it_extracts_forwarded_ip_with_quotes() {
        let mut req = create_request_with_trusted_proxy();
        req.headers_mut().insert(
            HeaderName::from_static("forwarded"),
            r#"for="192.168.1.1""#.parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_extracts_forwarded_ipv6_in_brackets() {
        let mut req = create_request_with_trusted_proxy();
        req.headers_mut().insert(
            HeaderName::from_static("forwarded"),
            r#"for="[2001:db8::1]""#.parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&"2001:db8::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn it_extracts_forwarded_ip_ignoring_port() {
        let mut req = create_request_with_trusted_proxy();
        req.headers_mut().insert(
            HeaderName::from_static("forwarded"),
            "for=192.168.1.1:1234".parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_extracts_forwarded_ipv6_with_port() {
        let mut req = create_request_with_trusted_proxy();
        req.headers_mut().insert(
            HeaderName::from_static("forwarded"),
            r#"for="[2001:db8::1]:8443""#.parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&"2001:db8::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn it_extracts_forwarded_from_multiple_entries_preferring_nearest_tail() {
        let mut req = create_request_with_trusted_proxy();
        req.headers_mut().insert(
            HeaderName::from_static("forwarded"),
            "for=10.0.0.1, for=192.168.1.1".parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_extracts_xff_from_list() {
        let mut req = create_request_with_trusted_proxy();
        req.headers_mut().insert(
            HeaderName::from_static("x-forwarded-for"),
            "10.0.0.1, 192.168.1.1".parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_ignores_xff_unknown_and_parses_next() {
        let mut req = create_request_with_trusted_proxy();
        req.headers_mut().insert(
            HeaderName::from_static("x-forwarded-for"),
            "unknown, 192.168.1.1".parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_ignores_xff_when_proxy_is_untrusted() {
        let mut req = create_request();
        req.headers_mut().insert(
            HeaderName::from_static("x-forwarded-for"),
            "192.168.1.1".parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(127_u32.into())));
    }

    #[test]
    fn it_selects_first_untrusted_before_trusted_proxies_from_xff_chain() {
        let mut req = create_request_with_specific_trusted_proxy_chain(
            /* peer */ "10.0.0.3".parse().unwrap(),
            /* trusted */ ["10.0.0.2".parse().unwrap(), "10.0.0.3".parse().unwrap()],
        );

        req.headers_mut().insert(
            HeaderName::from_static("x-forwarded-for"),
            "192.168.1.1, 10.0.0.2".parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_falls_back_when_forwarded_header_is_too_long() {
        let mut req = create_request_with_trusted_proxy();
        let huge = "for=192.168.1.1;proto=https,".repeat(10_000);
        req.headers_mut().insert(
            HeaderName::from_static("forwarded"),
            huge.parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(127_u32.into())));
    }

    #[test]
    fn it_caps_xff_chain_to_max_ips_using_right_tail() {
        let mut req = create_request_with_trusted_proxy();
        let mut parts = vec![];
        for i in 0..50 {
            parts.push(format!("10.0.0.{i}"));
        }
        parts.push("192.168.1.1".into());
        let header = parts.join(", ");

        req.headers_mut().insert(
            HeaderName::from_static("x-forwarded-for"),
            header.parse().unwrap(),
        );

        let key = extract_partition_key_from_ip(&req).unwrap();
        assert_eq!(key, stable_hash(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn it_tests_stable_hash() {
        let key = stable_hash(&IpAddr::V4(127_u32.into()));
        assert_eq!(key, stable_hash(&IpAddr::V4(127_u32.into())));
    }
    
    #[test]
    fn it_adds_default_fixed_window_policy() {
        let mut global_limiter = GlobalRateLimiter {
            ..Default::default()
        };
        
        global_limiter.add_fixed_window(
            FixedWindow::new(10, Duration::from_secs(10))
        );
        
        let default = global_limiter.fixed_window(None).unwrap();
        
        assert_eq!(default.max_requests(), 10);
        assert_eq!(default.window_size_secs(), 10);
    }

    #[test]
    fn it_adds_default_sliding_window_policy() {
        let mut global_limiter = GlobalRateLimiter {
            ..Default::default()
        };

        global_limiter.add_sliding_window(
            SlidingWindow::new(10, Duration::from_secs(10))
        );

        let default = global_limiter.sliding_window(None).unwrap();

        assert_eq!(default.max_requests(), 10);
        assert_eq!(default.window_size_secs(), 10);
    }

    #[test]
    fn it_adds_named_fixed_window_policy() {
        let mut global_limiter = GlobalRateLimiter {
            ..Default::default()
        };

        global_limiter.add_fixed_window(
            FixedWindow::new(10, Duration::from_secs(10))
                .with_name("burst")
        );

        assert!(global_limiter.default_fixed_window.is_none());

        let default = global_limiter.fixed_window(Some("burst")).unwrap();
        
        assert_eq!(default.max_requests(), 10);
        assert_eq!(default.window_size_secs(), 10);
    }

    #[test]
    fn it_adds_named_sliding_window_policy() {
        let mut global_limiter = GlobalRateLimiter {
            ..Default::default()
        };

        global_limiter.add_sliding_window(
            SlidingWindow::new(10, Duration::from_secs(10))
                .with_name("burst")
        );

        assert!(global_limiter.default_sliding_window.is_none());

        let default = global_limiter.sliding_window(Some("burst")).unwrap();

        assert_eq!(default.max_requests(), 10);
        assert_eq!(default.window_size_secs(), 10);
    }
    
    #[test]
    fn it_add_fixed_window_policy() {
        let app = App::new()
            .with_fixed_window(FixedWindow::new(10, Duration::from_secs(10)));
        
        let limiter = app.rate_limiter
            .unwrap()
            .default_fixed_window
            .unwrap();
        
        assert_eq!(limiter.max_requests(), 10);
        assert_eq!(limiter.window_size_secs(), 10);
    }

    #[test]
    fn it_add_named_fixed_window_policy() {
        let app = App::new()
            .with_fixed_window(
                FixedWindow::new(10, Duration::from_secs(10))
                    .with_name("burst")
            );

        let global_limiter = app.rate_limiter.unwrap();
        let limiter = global_limiter
            .fixed_window(Some("burst"))
            .unwrap();

        assert_eq!(limiter.max_requests(), 10);
        assert_eq!(limiter.window_size_secs(), 10);
    }

    #[test]
    fn it_add_sliding_window_policy() {
        let app = App::new()
            .with_sliding_window(SlidingWindow::new(10, Duration::from_secs(10)));

        let limiter = app.rate_limiter
            .unwrap()
            .default_sliding_window
            .unwrap();

        assert_eq!(limiter.max_requests(), 10);
        assert_eq!(limiter.window_size_secs(), 10);
    }

    #[test]
    fn it_add_named_sliding_window_policy() {
        let app = App::new()
            .with_sliding_window(
                SlidingWindow::new(10, Duration::from_secs(10))
                    .with_name("burst")
            );

        let global_limiter = app.rate_limiter.unwrap();
        let limiter = global_limiter
            .sliding_window(Some("burst"))
            .unwrap();

        assert_eq!(limiter.max_requests(), 10);
        assert_eq!(limiter.window_size_secs(), 10);
    }
}

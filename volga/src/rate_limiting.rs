//! Tools and utilities for Rate Limiting

use twox_hash::XxHash64;
use smallvec::SmallVec;
use std::{
    hash::{Hash, Hasher}, 
    net::{IpAddr, SocketAddr}, 
    sync::Arc, 
    time::Duration
};
use std::fmt::{Debug, Formatter};
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

pub use volga_rate_limiter::{
    FixedWindowRateLimiter,
    SlidingWindowRateLimiter,
    RateLimiter
};

pub mod by;

const X_FORWARDED_FOR: &str = "x-forwarded-for";
const DEFAULT_POLICIES_COUNT: usize = 4;

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
    key: Arc<dyn RateLimitKey>,

    /// Optional policy name selecting a specific rate-limiting configuration.
    ///
    /// If `None`, the default policy for the given rate-limiting algorithm
    /// is used.
    policy: Option<PolicyName>,
}

impl Debug for RateLimitBinding {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitBinding(...)").finish()
    }
}

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

/// Configuration for a **fixed window** rate limiting policy.
///
/// This struct defines the policy parameters:
/// - `max_requests` — maximum number of requests allowed per window
/// - `window_size` — duration of a single fixed window
/// - `eviction` — optional duration after which the data for inactive clients is cleaned up
/// - `name` — optional name to identify a named policy
#[derive(Debug, Clone)]
pub struct FixedWindow {
    /// Optional name of the policy
    name: Option<String>,
    
    /// Maximum number of requests allowed in the window
    max_requests: u32,
    
    /// Duration of the window
    window_size: Duration,
    
    /// Optional eviction period
    eviction: Option<Duration>,
}

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
    name: Option<String>,
    
    /// Maximum number of requests allowed in the window
    max_requests: u32,
    
    /// Duration of the window
    window_size: Duration,
    
    /// Optional eviction period
    eviction: Option<Duration>,
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

impl FixedWindow {
    /// Creates a new fixed window rate limiting policy.
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
            window_size,
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

    /// Builds a `FixedWindowRateLimiter` instance based on this policy.
    #[inline]
    fn build(&self) -> FixedWindowRateLimiter {
        let mut limiter = FixedWindowRateLimiter::new(
            self.max_requests,
            self.window_size
        );

        if let Some(eviction) = self.eviction {
            limiter.set_eviction(eviction);
        }

        limiter
    }
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
    fn build(&self) -> SlidingWindowRateLimiter {
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

impl App {
    /// Sets the fixed window rate limiter
    pub fn with_fixed_window(mut self, policy: FixedWindow) -> Self {
        self.rate_limiter
            .get_or_insert_default()
            .add_fixed_window(policy);
        self
    }

    /// Sets the sliding window rate limiter
    pub fn with_sliding_window(mut self, policy: SlidingWindow) -> Self {
        self.rate_limiter
            .get_or_insert_default()
            .add_sliding_window(policy);
        self
    }

    /// Adds the global middleware that limits all requests
    pub fn use_fixed_window<K: RateLimitKeyExt>(&mut self, source: K) -> &mut Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_fixed_window(ctx, binding.clone(), next))
    }

    /// Adds the global middleware that limits all requests
    pub fn use_sliding_window<K: RateLimitKeyExt>(&mut self, source: K) -> &mut Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_sliding_window(ctx, binding.clone(), next))
    }
}

impl<'a> Route<'a> {
    /// Adds the middleware that limits all requests for this route
    pub fn fixed_window<K: RateLimitKeyExt>(self, source: K) -> Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_fixed_window(ctx, binding.clone(), next))
    }

    /// Adds the middleware that limits all requests for this route
    pub fn sliding_window<K: RateLimitKeyExt>(self, source: K) -> Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_sliding_window(ctx, binding.clone(), next))
    }
}

impl<'a> RouteGroup<'a> {
    /// Adds the middleware that limits all requests for this route group
    pub fn fixed_window<K: RateLimitKeyExt>(self, source: K) -> Self {
        let binding = source.bind();
        self.wrap(move |ctx, next| check_fixed_window(ctx, binding.clone(), next))
    }

    /// Adds the middleware that limits all requests for this route group
    pub fn sliding_window<K: RateLimitKeyExt>(self, source: K) -> Self {
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
                "Rate limit exceeded. Try again later."
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
                "Rate limit exceeded. Try again later."
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
    header
        .split(',')
        .next()
        .map(str::trim)
        .and_then(|ip| ip.parse::<IpAddr>().ok())
}

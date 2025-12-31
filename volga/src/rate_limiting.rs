//! Tools and utilities for Rate Limiting

use std::time::Duration;
use twox_hash::XxHash64;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, SocketAddr};
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

/// Represents a fixed window rate limiter policy
#[derive(Debug, Clone, Copy)]
pub struct FixedWindow {
    max_requests: u32,
    window_size: Duration,
    eviction: Option<Duration>
}

/// Represents a sliding window rate limiter policy
#[derive(Debug, Clone, Copy)]
pub struct SlidingWindow {
    max_requests: u32,
    window_size: Duration,
    eviction: Option<Duration>
}

/// Defines how a rate-limiting partition key is extracted from an HTTP request.
///
/// Implementations of this trait determine how requests are grouped
/// for the purposes of rate limiting.
///
/// The extracted key must be:
/// - Stable for the same logical client
/// - Fast to compute
/// - Safe to use concurrently
pub trait RateLimitKey: Clone + Send + Sync {
    /// Extracts a partition key from the given HTTP request.
    ///
    /// Implementations should return a stable `u64` value that uniquely
    /// identifies a client or logical request group.
    fn extract(&self, req: &HttpRequest) -> Result<u64, Error>;
}

/// Global rate limiter
#[derive(Debug, Default)]
pub struct GlobalRateLimiter {
    pub(crate) fixed_window: Option<FixedWindowRateLimiter>,
    pub(crate) sliding_window: Option<SlidingWindowRateLimiter>
}

impl FixedWindow {
    /// Creates a new fixed window rate limiting policy
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self {
            eviction: None,
            max_requests,
            window_size,
        }
    }

    /// Sets the eviction period
    #[inline]
    pub fn with_eviction(mut self, eviction: Duration) -> Self {
        self.eviction = Some(eviction);
        self
    }

    /// Builds a fixed window rate limiter based on policy
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
    /// Creates a new fixed window rate limiting policy
    #[inline]
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self {
            eviction: None,
            max_requests,
            window_size
        }
    }

    /// Sets the eviction period
    #[inline]
    pub fn with_eviction(mut self, eviction: Duration) -> Self {
        self.eviction = Some(eviction);
        self
    }

    /// Builds a sliding window rate limiter based on policy
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
            .fixed_window = Some(policy.build());
        self
    }

    /// Sets the sliding window rate limiter
    pub fn with_sliding_window(mut self, policy: SlidingWindow) -> Self {
        self.rate_limiter
            .get_or_insert_default()
            .sliding_window = Some(policy.build());
        self
    }

    /// Adds the global middleware that limits all requests
    pub fn use_fixed_window(&mut self, source: impl RateLimitKey + 'static) -> &mut Self {
        self.wrap(move |ctx, next| check_fixed_window(ctx, source.clone(), next))
    }

    /// Adds the global middleware that limits all requests
    pub fn use_sliding_window(&mut self, source: impl RateLimitKey+ 'static) -> &mut Self {
        self.wrap(move |ctx, next| check_sliding_window(ctx, source.clone(), next))
    }
}

impl<'a> Route<'a> {
    /// Adds the middleware that limits all requests for this route
    pub fn fixed_window(self, source: impl RateLimitKey+ 'static) -> Self {
        self.wrap(move |ctx, next| check_fixed_window(ctx, source.clone(), next))
    }

    /// Adds the middleware that limits all requests for this route
    pub fn sliding_window(self, source: impl RateLimitKey+ 'static) -> Self {
        self.wrap(move |ctx, next| check_sliding_window(ctx, source.clone(), next))
    }
}

impl<'a> RouteGroup<'a> {
    /// Adds the middleware that limits all requests for this route group
    pub fn fixed_window(self, source: impl RateLimitKey+ 'static) -> Self {
        self.wrap(move |ctx, next| check_fixed_window(ctx, source.clone(), next))
    }

    /// Adds the middleware that limits all requests for this route group
    pub fn sliding_window(self, source: impl RateLimitKey + 'static) -> Self {
        self.wrap(move |ctx, next| check_sliding_window(ctx, source.clone(), next))
    }
}

#[inline]
async fn check_fixed_window(ctx: HttpContext, source: impl RateLimitKey, next: NextFn) -> HttpResult {
    if let Some(limiter) = ctx.fixed_window_rate_limiter() {
        let key = source.extract(&ctx.request)?;
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
async fn check_sliding_window(ctx: HttpContext, source: impl RateLimitKey, next: NextFn) -> HttpResult {
    if let Some(limiter) = ctx.sliding_window_rate_limiter() {
        let key = source.extract(&ctx.request)?;
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

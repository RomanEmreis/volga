//! Tools and utilities for Rate Limiting

use std::time::Duration;
use twox_hash::XxHash64;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, SocketAddr};
use crate::{
    App,
    ClientIp,
    HttpRequest,
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

const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// Represents a source where the 
/// rate limiting partition key calculated from
#[derive(Debug, Clone, Copy)]
pub enum PartitionKeySource {
    /// Partition Key will be calculated from the "forwarded" header, 
    /// if it's not present then from "x-forwarded-for". Fallback - the peer address.
    Ip,
    /// Partition Key will be calculated from the User Identity,
    #[cfg(feature = "jwt-auth")]
    User,
    //Custom()
}

/// Global rate limiter
#[derive(Debug, Default)]
pub struct GlobalRateLimiter {
    pub(crate) fixed_window: Option<FixedWindowRateLimiter>,
    pub(crate) sliding_window: Option<SlidingWindowRateLimiter>
}

impl App {
    /// Sets the fixed window rate limiter
    pub fn with_fixed_window(&mut self, max_requests: u32, window_size: Duration) -> &mut Self {
        self.rate_limiter
            .get_or_insert_default()
            .fixed_window = Some(FixedWindowRateLimiter::new(max_requests, window_size));
        self
    }

    /// Sets the sliding window rate limiter
    pub fn with_sliding_window(&mut self, max_requests: u32, window_size: Duration) -> &mut Self {
        self.rate_limiter
            .get_or_insert_default()
            .sliding_window = Some(SlidingWindowRateLimiter::new(max_requests, window_size));
        self
    }

    /// Adds the global middleware that limits all requests
    pub fn use_fixed_window(&mut self, source: PartitionKeySource) -> &mut Self {
        self.wrap(move |ctx, next| async move {
            if let Some(limiter) = ctx.fixed_window_rate_limiter() {
                let key = extract_partition_key(&ctx.request, source)?;
                if limiter.check(key) { 
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
        })
    }

    /// Adds the global middleware that limits all requests
    pub fn use_sliding_window(&mut self, source: PartitionKeySource) -> &mut Self {
        self.wrap(move |ctx, next| async move {
            if let Some(limiter) = ctx.sliding_window_rate_limiter() {
                let key = extract_partition_key(&ctx.request, source)?;
                if limiter.check(key) {
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
        })
    }
}

#[inline]
fn extract_partition_key(req: &HttpRequest, source: PartitionKeySource) -> Result<u64, Error> {
    match source {
        PartitionKeySource::Ip => extract_partition_key_from_ip(req),
        //PartitionKeySource::Custom(extractor) => extractor(req).map(stable_hash),
        #[cfg(feature = "jwt-auth")]
        PartitionKeySource::User => extract_partition_key_from_auth(req)
    }
}

#[inline]
fn extract_partition_key_from_ip(req: &HttpRequest) -> Result<u64, Error> {
    let ip = req.extract::<ClientIp>()?;
    let client_ip = extract_client_ip(req, ip.into_inner());
    Ok(stable_hash(&client_ip))
}

#[inline]
fn extract_partition_key_from_auth(_req: &HttpRequest) -> Result<u64, Error> {
    todo!()
}

#[inline]
fn stable_hash<T: Hash>(value: &T) -> u64 {
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

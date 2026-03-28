//! Per-request data consolidated into a single extension entry.
//!
//! [`HttpRequestScope`] replaces N individual `Box<dyn Any>` heap allocations
//! (one per extension insert) with a single allocation per request.

use tokio_util::sync::CancellationToken;

use crate::{
    ClientIp,
    http::{endpoints::route::PathArgs, request::request_body_limit::RequestBodyLimit},
};

#[cfg(any(
    feature = "rate-limiting",
    feature = "config",
    all(test, feature = "ws")
))]
use std::sync::Arc;

#[cfg(feature = "ws")]
use crate::error::handler::WeakErrorHandler;

#[cfg(feature = "jwt-auth")]
use crate::auth::bearer::BearerTokenService;

#[cfg(any(
    feature = "decompression-brotli",
    feature = "decompression-gzip",
    feature = "decompression-zstd",
    feature = "decompression-full"
))]
use crate::middleware::decompress::ResolvedDecompressionLimits;

#[cfg(feature = "rate-limiting")]
use {
    crate::rate_limiting::GlobalRateLimiter,
    std::{collections::HashSet, net::IpAddr},
};

#[cfg(feature = "config")]
use crate::config::store::ConfigStore;

/// Consolidates all per-request data previously stored as individual extension
/// entries into a single heap allocation.
///
/// Replaces N separate `Box<dyn Any>` allocations and hashmap inserts with one,
/// inserted as a single extension entry per matched request.
#[derive(Debug, Clone)]
pub(crate) struct HttpRequestScope {
    /// The client's IP address.
    pub(crate) client_ip: ClientIp,

    /// Cooperative cancellation token for this request.
    pub(crate) cancellation_token: CancellationToken,

    /// Request body size limit.
    pub(crate) body_limit: RequestBodyLimit,

    /// Route path parameters. Consumed once by the extractor dispatch via
    /// `std::mem::take` on the mutable extensions reference.
    pub(crate) params: PathArgs,

    /// Weak reference to the error handler, used by WebSocket connections.
    #[cfg(feature = "ws")]
    pub(crate) error_handler: WeakErrorHandler,

    /// JWT bearer token service, `None` when auth is not configured.
    #[cfg(feature = "jwt-auth")]
    pub(crate) bearer_token_service: Option<BearerTokenService>,

    /// Resolved limits for the decompression middleware.
    #[cfg(any(
        feature = "decompression-brotli",
        feature = "decompression-gzip",
        feature = "decompression-zstd",
        feature = "decompression-full"
    ))]
    pub(crate) decompression_limits: ResolvedDecompressionLimits,

    /// Global rate limiter, `None` when rate limiting is not configured.
    #[cfg(feature = "rate-limiting")]
    pub(crate) rate_limiter: Option<Arc<GlobalRateLimiter>>,

    /// Trusted proxy IPs for client IP extraction, `None` when not configured.
    #[cfg(feature = "rate-limiting")]
    pub(crate) trusted_proxies: Option<Arc<HashSet<IpAddr>>>,

    /// Pre-deserialized config sections, `None` when config is not loaded.
    #[cfg(feature = "config")]
    pub(crate) config: Option<Arc<ConfigStore>>,
}

#[cfg(test)]
impl Default for HttpRequestScope {
    fn default() -> Self {
        use std::net::SocketAddr;
        Self {
            client_ip: ClientIp(SocketAddr::from(([0, 0, 0, 0], 0))),
            cancellation_token: CancellationToken::new(),
            body_limit: RequestBodyLimit::Disabled,
            params: PathArgs::default(),
            #[cfg(feature = "ws")]
            error_handler: {
                use crate::error::{ErrorFunc, handler::PipelineErrorHandler};
                let h =
                    PipelineErrorHandler::from(ErrorFunc::new(|_: crate::error::Error| async {}));
                Arc::downgrade(&h)
            },
            #[cfg(feature = "jwt-auth")]
            bearer_token_service: None,
            #[cfg(any(
                feature = "decompression-brotli",
                feature = "decompression-gzip",
                feature = "decompression-zstd",
                feature = "decompression-full"
            ))]
            decompression_limits: crate::middleware::decompress::DecompressionLimits::default()
                .resolved(),
            #[cfg(feature = "rate-limiting")]
            rate_limiter: None,
            #[cfg(feature = "rate-limiting")]
            trusted_proxies: None,
            #[cfg(feature = "config")]
            config: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn make_scope() -> HttpRequestScope {
        HttpRequestScope {
            client_ip: ClientIp(SocketAddr::from(([127, 0, 0, 1], 4321))),
            cancellation_token: CancellationToken::new(),
            body_limit: RequestBodyLimit::Enabled(1024),
            params: PathArgs::default(),
            ..HttpRequestScope::default()
        }
    }

    #[test]
    fn it_stores_client_ip() {
        let scope = make_scope();
        assert_eq!(scope.client_ip.0, SocketAddr::from(([127, 0, 0, 1], 4321)));
    }

    #[test]
    fn it_stores_body_limit() {
        let scope = make_scope();
        assert_eq!(scope.body_limit, RequestBodyLimit::Enabled(1024));
    }

    #[test]
    fn it_stores_cancellation_token() {
        let scope = make_scope();
        scope.cancellation_token.cancel();
        assert!(scope.cancellation_token.is_cancelled());
    }

    #[test]
    fn it_can_be_inserted_into_extensions() {
        use hyper::http::Extensions;
        let scope = make_scope();
        let mut ext = Extensions::new();
        ext.insert(scope.clone());
        let retrieved = ext.get::<HttpRequestScope>().unwrap();
        assert_eq!(retrieved.client_ip.0, scope.client_ip.0);
    }
}

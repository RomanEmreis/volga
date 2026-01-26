//! Types and utils for control the Application Environment and runtime.

use super::App;
use super::pipeline::Pipeline;
use hyper_util::server::graceful::GracefulShutdown;
use std::net::IpAddr;
use crate::{
    http::request::request_body_limit::RequestBodyLimit,
    headers::HeaderValue,
    Limit
};

use std::{
    io::Error,
    sync::Arc
};

#[cfg(feature = "di")]
use crate::di::Container;

#[cfg(feature = "tls")]
use {
    crate::tls::HstsHeader,
    tokio_rustls::TlsAcceptor
};

#[cfg(feature = "tracing")]
use crate::tracing::TracingConfig;

#[cfg(feature = "middleware")]
use crate::http::cors::CorsRegistry;

#[cfg(feature = "jwt-auth")]
use crate::auth::bearer::BearerTokenService;

#[cfg(feature = "rate-limiting")]
use {
    crate::rate_limiting::GlobalRateLimiter,
    std::collections::HashSet
};

#[cfg(feature = "static-files")]
use super::host_env::HostEnv;

#[cfg(feature = "http2")]
use crate::limits::Http2Limits;

#[cfg(any(
    feature = "decompression-brotli",
    feature = "decompression-gzip",
    feature = "decompression-zstd",
    feature = "decompression-full"
))]
use crate::middleware::decompress::ResolvedDecompressionLimits;

pub(crate) const GRACEFUL_SHUTDOWN_TIMEOUT: u64 = 10;

/// The application runtime environment, formed from [`App`].
///
/// Stores immutable settings and shared Web Server resources
/// (pipeline, limits, TLS/DI/tracing, rate limiter, etc.),
/// which are created once at startup and shared
/// by all connections/requests.
pub(crate) struct AppEnv {
    /// Maximum total size (in bytes) of HTTP headers per request.
    pub(crate) max_header_size: Limit<usize>,

    /// Maximum number of HTTP headers per request.
    pub(crate) max_header_count: Limit<usize>,

    /// Graceful shutdown utilities
    pub(crate) graceful_shutdown: GracefulShutdown,

    /// Request/Middleware pipeline
    pub(super) pipeline: Pipeline,

    /// Default `Cache-Control` header value
    pub(super) cache_control: Option<HeaderValue>,
    
    /// Request body limit
    pub(super) body_limit: RequestBodyLimit,

    /// HTTP/2 resource and backpressure limits.
    #[cfg(feature = "http2")]
    pub(crate) http2_limits: Http2Limits,
    
    /// Incoming TLS connection acceptor
    #[cfg(feature = "tls")]
    pub(crate) acceptor: Option<TlsAcceptor>,

    /// Web Server's Hosting Environment
    #[cfg(feature = "static-files")]
    pub(super) host_env: HostEnv,

    /// Service that validates/generates JWTs
    #[cfg(feature = "jwt-auth")]
    pub(super) bearer_token_service: Option<BearerTokenService>,

    /// Global rate limiter
    #[cfg(feature = "rate-limiting")]
    pub(super) rate_limiter: Option<Arc<GlobalRateLimiter>>,

    /// Trusted proxies for rate limiting IP extraction
    #[cfg(feature = "rate-limiting")]
    pub(super) trusted_proxies: Option<Arc<HashSet<IpAddr>>>,

    /// HSTS configuration options
    #[cfg(feature = "tls")]
    pub(super) hsts: Option<HstsHeader>,

    /// Tracing configuration options
    #[cfg(feature = "tracing")]
    pub(super) tracing_config: Option<TracingConfig>,

    /// Limits for decompression middleware
    #[cfg(any(
        feature = "decompression-brotli",
        feature = "decompression-gzip",
        feature = "decompression-zstd",
        feature = "decompression-full"
    ))]
    pub(super) decompression_limits: ResolvedDecompressionLimits,

    /// CORS registry
    #[cfg(feature = "middleware")]
    pub(super) cors: CorsRegistry,

    /// Dependency Injection container
    #[cfg(feature = "di")]
    pub(super) container: Container,
}

impl TryFrom<App> for AppEnv {
    type Error = Error;

    fn try_from(app: App) -> Result<Self, Self::Error> {
        #[cfg(feature = "tls")]
        let hsts = app.tls_config
            .as_ref()
            .map(|tls| HstsHeader::new(tls.hsts_config.clone()));

        #[cfg(feature = "tls")]
        let acceptor = {
            let tls_config = app.tls_config
                .map(|config| config.build())
                .transpose()?;
            tls_config
                .map(|config| TlsAcceptor::from(Arc::new(config)))
        };

        #[cfg(feature = "jwt-auth")]
        let bearer_token_service = app.auth_config.map(Into::into);

        let default_cache_control = app.cache_control
            .map(|c| c.try_into())
            .transpose()?;

        let app_instance = Self {
            body_limit: app.body_limit,
            pipeline: app.pipeline.build(),
            graceful_shutdown: GracefulShutdown::new(),
            max_header_count: app.max_header_count,
            max_header_size: app.max_header_size,
            cache_control: default_cache_control,
            #[cfg(any(
                feature = "decompression-brotli",
                feature = "decompression-gzip",
                feature = "decompression-zstd",
                feature = "decompression-full"
            ))]
            decompression_limits: app.decompression_limits.resolved(),
            #[cfg(feature = "http2")]
            http2_limits: app.http2_limits,
            #[cfg(feature = "middleware")]
            cors: app.cors,
            #[cfg(feature = "static-files")]
            host_env: app.host_env,
            #[cfg(feature = "di")]
            container: app.container.build(),
            #[cfg(feature = "rate-limiting")]
            rate_limiter: app.rate_limiter.map(Arc::new),
            #[cfg(feature = "rate-limiting")]
            trusted_proxies: app.trusted_proxies.map(Arc::new),
            #[cfg(feature = "jwt-auth")]
            bearer_token_service,
            #[cfg(feature = "tracing")]
            tracing_config: app.tracing_config,
            #[cfg(feature = "tls")]
            acceptor,
            #[cfg(feature = "tls")]
            hsts
        };
        Ok(app_instance)
    }
}

impl AppEnv {
    /// Gracefully shutdown current instance
    #[inline]
    pub(super) async fn shutdown(self) {
        tokio::select! {
            _ = self.graceful_shutdown.shutdown() => {
                #[cfg(feature = "tracing")]
                tracing::info!("shutting down the server...");
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(GRACEFUL_SHUTDOWN_TIMEOUT)) => {
                #[cfg(feature = "tracing")]
                tracing::warn!("timed out wait for all connections to close");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_converts_into_app_env() {
        let app = App::default();

        let app_instance: AppEnv = app.try_into().unwrap();

        let RequestBodyLimit::Enabled(limit) = app_instance.body_limit else { unreachable!() };
        assert_eq!(limit, 5242880);
        
        assert_eq!(app_instance.max_header_count, Limit::Default);
        assert_eq!(app_instance.max_header_size, Limit::Default);
    }
}
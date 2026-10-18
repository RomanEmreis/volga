//! Types and utils for control the Application Environment and runtime.

use super::App;
use super::pipeline::Pipeline;
use crate::{
    Limit, ShutdownHandle, headers::HeaderValue,
    http::request::request_body_limit::RequestBodyLimit,
};
use std::io::Error;

#[cfg(any(
    feature = "tls",
    feature = "rate-limiting",
    feature = "config",
    feature = "oauth-client"
))]
use std::sync::Arc;

#[cfg(feature = "config")]
use crate::config::ConfigStore;

#[cfg(feature = "di")]
use crate::di::{
    Container,
    error::{Issue, ValidationError},
};

#[cfg(feature = "tls")]
use {crate::tls::HstsHeader, tokio_rustls::TlsAcceptor};

#[cfg(feature = "tracing")]
use crate::tracing::TracingConfig;

#[cfg(feature = "middleware")]
use crate::http::cors::CorsRegistry;

#[cfg(feature = "jwt-auth")]
use crate::auth::bearer::BearerTokenService;

#[cfg(feature = "rate-limiting")]
use {
    crate::rate_limiting::GlobalRateLimiter,
    std::{collections::HashSet, net::IpAddr},
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

    /// Request/Middleware pipeline
    pub(super) pipeline: Pipeline,

    /// Default `Cache-Control` header value
    pub(super) cache_control: Option<HeaderValue>,

    /// Request body limit
    pub(super) body_limit: RequestBodyLimit,

    /// Shutdown handle of the running server, handed to the requests that extract it
    pub(super) shutdown: ShutdownHandle,

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

    /// Pre-deserialized user config sections
    #[cfg(feature = "config")]
    pub(super) config: Option<Arc<ConfigStore>>,
}

impl TryFrom<App> for AppEnv {
    type Error = Error;

    fn try_from(app: App) -> Result<Self, Self::Error> {
        // A dependency graph that cannot be resolved - a cycle, or a service, handler or
        // middleware depending on one nobody registered - stops the app here, before a
        // connection is accepted, rather than the first request that reaches it
        #[cfg(all(feature = "di", feature = "middleware"))]
        let app = app.declare_group_requirements();
        #[cfg(feature = "di")]
        app.container.validate().map_err(startup_error)?;

        #[cfg(feature = "tls")]
        let hsts = app
            .tls_config
            .as_ref()
            .map(|tls| HstsHeader::new(tls.hsts_config.clone()));

        #[cfg(all(feature = "jwt-auth", feature = "tls"))]
        let tls_enabled = app.tls_config.is_some();
        #[cfg(all(feature = "jwt-auth", not(feature = "tls")))]
        let tls_enabled = false;

        #[cfg(feature = "tls")]
        let acceptor = {
            let tls_config = app.tls_config.map(|config| config.build()).transpose()?;
            tls_config.map(|config| TlsAcceptor::from(Arc::new(config)))
        };

        #[cfg(feature = "jwt-auth")]
        let bearer_token_service = {
            let derived_metadata_url = app.oauth_resource_metadata_url;

            #[cfg(feature = "oauth-client")]
            let mut auth_config = app.auth_config;
            #[cfg(not(feature = "oauth-client"))]
            let auth_config = app.auth_config;

            #[cfg(feature = "oauth-client")]
            let oauth_store = match (app.oauth_client_enabled, app.oauth_client_config) {
                (true, Some(oauth)) => {
                    let issuer = oauth
                        .issuer
                        .clone()
                        .expect("`use_oauth` verified the issuer is present");
                    // issuer-based validation works without an explicit
                    // bearer config; the defaults apply
                    let cfg = auth_config.get_or_insert_with(Default::default);
                    if cfg.decoding_key().is_some() {
                        return Err(Error::other(
                            "Bearer auth is ambiguous: both a static decoding key and an \
                             OAuth issuer are configured - pick one",
                        ));
                    }
                    // tokens must come from the configured issuer unless
                    // the application constrained `iss` itself
                    cfg.set_default_issuer(&issuer);
                    Some(Arc::new(oauth.into_store()))
                }
                (false, Some(oauth)) if oauth.issuer.is_some() => {
                    return Err(Error::other(
                        "OAuth issuer is configured but not active; call `use_oauth()` to enable it",
                    ));
                }
                _ => None,
            };

            auth_config
                .map(|cfg| {
                    // a static key that cannot serve the configured algorithms
                    // would refuse every token - stop here instead
                    cfg.check_keys().map_err(Error::other)?;
                    let service = crate::auth::BearerTokenService::from_config(
                        cfg,
                        tls_enabled,
                        derived_metadata_url,
                    );
                    #[cfg(feature = "oauth-client")]
                    let service = match oauth_store {
                        Some(store) => service.with_jwks_store(store),
                        None => service,
                    };
                    Ok::<_, Error>(service)
                })
                .transpose()?
        };

        let default_cache_control = app.cache_control.map(|c| c.try_into()).transpose()?;

        let app_instance = Self {
            body_limit: app.body_limit,
            shutdown: app.shutdown_handle.unwrap_or_default(),
            pipeline: app.pipeline.build(),
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
            hsts,
            #[cfg(feature = "config")]
            config: app.config_store,
        };
        Ok(app_instance)
    }
}

/// What `App::run` fails with when the dependency graph does not resolve, with what to do
/// about a service that handlers or middleware take and nobody registered
#[cfg(feature = "di")]
fn startup_error(err: ValidationError) -> Error {
    let unregistered = err
        .issues()
        .iter()
        .any(|issue| matches!(issue, Issue::Unregistered { .. }));

    if unregistered {
        Error::other(format!(
            "{err}. Register a service with `add_singleton`, `add_scoped` or `add_transient` \
             (or their `_factory` and `_default` forms), or take `Option<Dc<T>>` where it is \
             optional"
        ))
    } else {
        Error::other(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_converts_into_app_env() {
        let app = App::default();

        let app_instance: AppEnv = app.try_into().unwrap();

        let RequestBodyLimit::Enabled(limit) = app_instance.body_limit else {
            unreachable!()
        };
        assert_eq!(limit, 5242880);

        assert_eq!(app_instance.max_header_count, Limit::Default);
        assert_eq!(app_instance.max_header_size, Limit::Default);
    }

    #[cfg(feature = "jwt-auth")]
    #[test]
    fn it_refuses_a_decoding_key_that_cannot_verify_the_configured_algs() {
        use crate::auth::{Algorithm, DecodingKey};

        // an HMAC secret cannot verify RS256 tokens
        let app = App::default().with_bearer_auth(|auth| {
            auth.set_decoding_key(DecodingKey::from_secret(b"secret"))
                .with_alg(Algorithm::RS256)
        });

        let Err(err) = AppEnv::try_from(app) else {
            panic!("a mismatched decoding key must stop the app");
        };
        assert_eq!(
            err.to_string(),
            "Bearer auth: the decoding key is an HMAC secret, which cannot verify RS256 tokens; \
             `with_alg` must name only HS256, HS384 or HS512"
        );
    }

    #[cfg(feature = "jwt-auth")]
    #[test]
    fn it_accepts_keys_that_suit_the_configured_algs() {
        use crate::auth::{Algorithm, DecodingKey, EncodingKey};

        let app = App::default().with_bearer_auth(|auth| {
            auth.set_encoding_key(EncodingKey::from_secret(b"secret"))
                .set_decoding_key(DecodingKey::from_secret(b"secret"))
                .with_alg(Algorithm::HS512)
        });

        let app_instance: AppEnv = app.try_into().unwrap();
        assert!(app_instance.bearer_token_service.is_some());
    }
}

//! Utilities for managing HTTP request scope

use crate::http::endpoints::args::FromRequestRef;
use crate::{
    HttpRequest, HttpRequestMut, HttpResult,
    app::pipeline::Terminal,
    error::Error,
    http::cors::{CorsHeaders, CorsOverride},
    status,
};

use hyper::header::ALLOW;
use std::sync::Arc;

#[cfg(feature = "di")]
use crate::di::Container;

#[cfg(feature = "rate-limiting")]
use crate::rate_limiting::{GlobalRateLimiter, RateLimiter};

#[cfg(feature = "rate-limiting")]
use crate::http::request_scope::HttpRequestScope;

/// Describes current HTTP context which consists of the current HTTP request data
/// and the reference to the method handler for this request
pub struct HttpContext {
    /// Current HTTP request
    request: HttpRequestMut,

    /// What answers this request once the middleware chain has run: the matched
    /// route's pipeline, the application fallback, or a `405`. `None` once the
    /// terminal has been taken, so a second execution has nothing left to run.
    terminal: Option<Terminal>,

    /// CORS headers for this route
    cors: CorsOverride,
}

impl std::fmt::Debug for HttpContext {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpContext(..)")
    }
}

impl HttpContext {
    /// Creates a new [`HttpContext`]
    #[inline]
    pub(crate) fn new(
        request: HttpRequest,
        terminal: Option<Terminal>,
        cors: CorsOverride,
    ) -> Self {
        Self {
            request: HttpRequestMut::new(request),
            terminal,
            cors,
        }
    }

    /// Splits [`HttpContext`] into request parts and the pipeline terminal
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn into_parts(self) -> (HttpRequestMut, Option<Terminal>, CorsOverride) {
        (self.request, self.terminal, self.cors)
    }

    /// Creates a new [`HttpContext`] from request parts and the pipeline terminal
    #[inline]
    pub(crate) fn from_parts(
        request: HttpRequestMut,
        terminal: Option<Terminal>,
        cors: CorsOverride,
    ) -> Self {
        Self {
            request,
            terminal,
            cors,
        }
    }

    /// Returns `true` when routing matched an endpoint for this request.
    ///
    /// The pipeline runs for every request, so a middleware also sees the ones
    /// that matched no route or matched a path but not a method - the fallback
    /// or a `405` answers those. Middleware that should only do its work for a
    /// real endpoint, or that answers on an endpoint's behalf, reads this to
    /// tell the two apart. It answers the same at every layer: a route's or a
    /// group's own middleware runs after the route pipeline has been taken and
    /// still sees `true`.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.wrap(|ctx, next| async move {
    ///     if !ctx.matched_route() {
    ///         // nothing to meter for a request no route will answer
    ///         return next(ctx).await;
    ///     }
    ///     next(ctx).await
    /// });
    ///# app.run().await
    ///# }
    /// ```
    #[inline]
    pub fn matched_route(&self) -> bool {
        matches!(
            self.terminal,
            Some(Terminal::Route(_) | Terminal::RouteTaken)
        )
    }

    /// Extracts a payload from request parts
    ///
    /// # Example
    /// ```no_run
    /// use volga::{middleware::HttpContext, Query};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Params {
    ///     id: u32,
    ///     key: String
    /// }
    ///
    /// # fn docs(ctx: HttpContext) -> std::io::Result<()> {
    /// let params: Query<Params> = ctx.extract()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn extract<T: FromRequestRef>(&self) -> Result<T, Error> {
        self.request.extract()
    }

    /// Returns a reference to the DI container of the request scope
    #[inline]
    #[cfg(feature = "di")]
    pub(crate) fn container(&self) -> Result<&Container, Error> {
        self.request.extensions().try_into().map_err(Into::into)
    }

    /// Resolves a service from Dependency Container as a clone, service must implement [`Clone`]
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve<T: Send + Sync + Clone + 'static>(&self) -> Result<T, Error> {
        self.container()?.resolve::<T>().map_err(Into::into)
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve_shared<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, Error> {
        self.container()?.resolve_shared::<T>().map_err(Into::into)
    }

    #[inline]
    #[cfg(feature = "rate-limiting")]
    fn rate_limiter(&self) -> Option<&Arc<GlobalRateLimiter>> {
        self.request
            .extensions()
            .get::<HttpRequestScope>()?
            .rate_limiter
            .as_ref()
    }

    /// Returns a reference to a Fixed Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub(crate) fn fixed_window_rate_limiter(
        &self,
        policy: Option<&str>,
    ) -> Option<&impl RateLimiter> {
        self.rate_limiter()?.fixed_window(policy)
    }

    /// Returns a reference to a Sliding Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub(crate) fn sliding_window_rate_limiter(
        &self,
        policy: Option<&str>,
    ) -> Option<&impl RateLimiter> {
        self.rate_limiter()?.sliding_window(policy)
    }

    /// Returns a reference to a Token Bucket Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub(crate) fn token_bucket_rate_limiter(
        &self,
        policy: Option<&str>,
    ) -> Option<&impl RateLimiter> {
        self.rate_limiter()?.token_bucket(policy)
    }

    /// Returns a reference to a GCRA Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub(crate) fn gcra_rate_limiter(&self, policy: Option<&str>) -> Option<&impl RateLimiter> {
        self.rate_limiter()?.gcra(policy)
    }

    /// Returns a read-only view of the request.
    ///
    /// This is the preferred way to inspect request data
    /// from middleware and extractors.
    #[inline]
    pub fn request(&self) -> &HttpRequest {
        self.request.as_read_only()
    }

    /// Returns a mutable request handle.
    ///
    /// Allows controlled mutation of request metadata.
    ///
    /// This method is intentionally explicit.
    #[inline]
    pub fn request_mut(&mut self) -> &mut HttpRequestMut {
        &mut self.request
    }

    /// Resolves effective CORS policy (Route > Group > Default)
    #[inline]
    pub(crate) fn resolve_cors(
        &self,
        default: Option<&Arc<CorsHeaders>>,
    ) -> Option<Arc<CorsHeaders>> {
        match &self.cors {
            CorsOverride::Named(cors) => Some(cors.clone()),
            CorsOverride::Inherit => default.cloned(),
            CorsOverride::Disabled => None,
        }
    }

    /// Executes the terminal stage of the pipeline for the current HTTP request
    #[inline]
    pub(crate) async fn execute(self) -> HttpResult {
        let (request, terminal, cors) = self.into_parts();
        match terminal {
            Some(Terminal::Route(pipeline)) => {
                pipeline
                    .call(Self {
                        request,
                        cors,
                        terminal: Some(Terminal::RouteTaken),
                    })
                    .await
            }
            Some(Terminal::Fallback(fallback)) => fallback.call(request.freeze()).await,
            Some(Terminal::MethodNotAllowed(allowed)) => status!(405; [
                (ALLOW, allowed.as_ref())
            ]),
            Some(Terminal::RouteTaken) | None => status!(405),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpBody;
    use hyper::Request;

    #[cfg(feature = "di")]
    use std::collections::HashMap;
    #[cfg(feature = "di")]
    use std::sync::Mutex;

    #[cfg(feature = "di")]
    use crate::di::ContainerBuilder;
    use crate::http::CorsConfig;

    #[cfg(feature = "di")]
    #[allow(dead_code)]
    #[derive(Clone, Default)]
    struct InMemoryCache {
        inner: Arc<Mutex<HashMap<String, String>>>,
    }

    fn create_ctx() -> HttpContext {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        HttpContext::new(
            HttpRequest::from_parts(parts, body),
            None,
            CorsOverride::Inherit,
        )
    }

    #[test]
    fn it_debugs() {
        let ctx = create_ctx();
        assert_eq!(format!("{ctx:?}"), "HttpContext(..)");
    }

    #[test]
    fn it_splits_into_parts() {
        let ctx = create_ctx();

        let (parts, _, _) = ctx.into_parts();

        assert_eq!(parts.uri(), "/")
    }

    #[test]
    #[cfg(feature = "di")]
    fn it_returns_err_if_there_is_no_di_container() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        let ctx = HttpContext::new(http_req, None, CorsOverride::Inherit);

        assert!(ctx.container().is_err());
    }

    #[test]
    #[cfg(feature = "di")]
    fn it_resolves_from_di_container() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());

        let req = Request::get("http://localhost/")
            .extension(container.build())
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        let ctx = HttpContext::new(http_req, None, CorsOverride::Inherit);

        let cache = ctx.resolve::<InMemoryCache>();

        assert!(cache.is_ok());
    }

    #[test]
    #[cfg(feature = "di")]
    fn it_resolves_shared_from_di_container() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());

        let req = Request::get("http://localhost/")
            .extension(container.build())
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        let ctx = HttpContext::new(http_req, None, CorsOverride::Inherit);

        let cache = ctx.resolve_shared::<InMemoryCache>();

        assert!(cache.is_ok());
    }

    #[test]
    fn it_resolves_cors() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let permissive_cors = CorsConfig::default()
            .with_name("permissive")
            .with_any_method()
            .with_any_header()
            .with_any_origin()
            .precompute();

        let ctx = HttpContext::new(
            http_req,
            None,
            CorsOverride::Named(Arc::new(permissive_cors)),
        );

        let resolved_cors = ctx.resolve_cors(None);

        assert!(resolved_cors.is_some());
    }

    #[test]
    fn it_resolves_default_cors() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let default_cors = CorsConfig::default()
            .with_methods(["GET", "POST"])
            .with_any_header()
            .with_any_origin()
            .precompute();

        let default_cors = Some(Arc::new(default_cors));

        let ctx = HttpContext::new(http_req, None, CorsOverride::Inherit);

        let resolved_cors = ctx.resolve_cors(default_cors.as_ref());

        assert!(resolved_cors.is_some());
    }

    #[test]
    fn it_resolves_disabled_cors() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let default_cors = CorsConfig::default()
            .with_methods(["GET", "POST"])
            .with_any_header()
            .with_any_origin()
            .precompute();

        let default_cors = Some(Arc::new(default_cors));

        let ctx = HttpContext::new(http_req, None, CorsOverride::Disabled);

        let resolved_cors = ctx.resolve_cors(default_cors.as_ref());

        assert!(resolved_cors.is_none());
    }
}

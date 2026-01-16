//! Utilities for managing HTTP request scope

use crate::http::endpoints::{
    route::RoutePipeline,
    args::FromRequestRef
};
use crate::{
    HttpRequest, HttpRequestMut, HttpResult,
    error::Error,
    http::cors::{CorsOverride, CorsHeaders},
    status
};

use std::sync::Arc;

#[cfg(feature = "di")]
use crate::di::Container;

#[cfg(feature = "rate-limiting")]
use crate::rate_limiting::{RateLimiter, GlobalRateLimiter};

/// Describes current HTTP context which consists of the current HTTP request data 
/// and the reference to the method handler for this request
pub struct HttpContext {
    /// Current HTTP request
    request: HttpRequestMut,
    
    /// Current route middleware pipeline or handler that mapped to handle the HTTP request
    pipeline: Option<RoutePipeline>,

    /// CORS headers for this route
    cors: CorsOverride
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
        pipeline: Option<RoutePipeline>,
        cors: CorsOverride
    ) -> Self {
        Self { 
            request: HttpRequestMut::new(request),
            pipeline,
            cors
        }
    }
    
    /// Splits [`HttpContext`] into request parts and pipeline
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn into_parts(self) -> (HttpRequestMut, Option<RoutePipeline>, CorsOverride) {
        (self.request, self.pipeline, self.cors)
    }

    /// Creates a new [`HttpContext`] from request parts and pipeline
    #[inline]
    pub(crate) fn from_parts(request: HttpRequestMut, pipeline: Option<RoutePipeline>, cors: CorsOverride) -> Self {
        Self { request, pipeline, cors }
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
        self.request
            .extensions()
            .try_into()
            .map_err(Into::into)
    }

    /// Resolves a service from Dependency Container as a clone, service must implement [`Clone`]
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve<T: Send + Sync + Clone + 'static>(&self) -> Result<T, Error> {
        self.container()?
            .resolve::<T>()
            .map_err(Into::into)
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve_shared<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, Error> {
        self.container()?
            .resolve_shared::<T>()
            .map_err(Into::into)
    }

    /// Returns a reference to a Fixed Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub(crate) fn fixed_window_rate_limiter(&self, policy: Option<&str>) -> Option<&impl RateLimiter> {
        self.request.extensions()
            .get::<Arc<GlobalRateLimiter>>()?
            .fixed_window(policy)
    }

    /// Returns a reference to a Sliding Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub(crate) fn sliding_window_rate_limiter(&self, policy: Option<&str>) -> Option<&impl RateLimiter> {
        self.request.extensions()
            .get::<Arc<GlobalRateLimiter>>()?
            .sliding_window(policy)
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
    pub(crate) fn resolve_cors(&self, default: Option<&Arc<CorsHeaders>>) -> Option<Arc<CorsHeaders>> {
        match &self.cors {
            CorsOverride::Named(cors) => Some(cors.clone()),
            CorsOverride::Inherit => default.cloned(),
            CorsOverride::Disabled => None,
        }
    }

    /// Executes the request handler for the current HTTP request
    #[inline]
    pub(crate) async fn execute(self) -> HttpResult {
        let (request, pipeline, cors) = self.into_parts();
        if let Some(pipeline) = pipeline {
            pipeline.call(Self { request, cors, pipeline: None }).await
        } else { 
            status!(405)
        }
    }
}

#[cfg(test)]
mod tests {
    use hyper::Request;
    use crate::HttpBody;
    use super::*;
    
    #[cfg(feature = "di")]
    use std::collections::HashMap;
    #[cfg(feature = "di")]
    use std::sync::Mutex;

    #[cfg(feature = "di")]
    use crate::di::ContainerBuilder;

    #[cfg(feature = "di")]
    #[allow(dead_code)]
    #[derive(Clone, Default)]
    struct InMemoryCache {
        inner: Arc<Mutex<HashMap<String, String>>>
    }
    
    fn create_ctx() -> HttpContext {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        HttpContext::new(
            HttpRequest::from_parts(parts, body),
            None,
            CorsOverride::Inherit
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
}

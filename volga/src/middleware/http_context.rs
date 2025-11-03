//! Utilities for managing HTTP request scope

use crate::http::endpoints::{
    route::RoutePipeline,
    args::FromRequestRef
};
use crate::{
    error::Error, 
    headers::{Header, FromHeaders},
    HttpRequest, HttpResult,
    status
};

#[cfg(any(feature = "tls", feature = "tracing"))]
use {
    crate::error::handler::WeakErrorHandler,
    hyper::http::request::Parts
};

#[cfg(any(feature = "tls", feature = "tracing", feature = "di"))]
use std::sync::Arc;

#[cfg(feature = "di")]
use crate::di::Inject;

/// Describes current HTTP context which consists of the current HTTP request data 
/// and the reference to the method handler for this request
pub struct HttpContext {
    /// Current HTTP request
    pub request: HttpRequest,
    /// Current route middleware pipeline or handler that mapped to handle the HTTP request
    pipeline: Option<RoutePipeline>
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
        pipeline: Option<RoutePipeline>
    ) -> Self {
        Self { request, pipeline }
    }

    /// Creates a new [`HttpContext`] with the route pipeline
    #[inline]
    pub(crate) fn with_pipeline(
        request: HttpRequest,
        pipeline: RoutePipeline
    ) -> Self {
        Self { request, pipeline: Some(pipeline) }
    }
    
    /// Creates a slim [`HttpContext`] that holds only the request information
    #[inline]
    pub(crate) fn slim(request: HttpRequest) -> Self {
        Self { request, pipeline: None }
    }
    
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn into_parts(self) -> (HttpRequest, Option<RoutePipeline>) {
        (self.request, self.pipeline)
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

    /// Resolves a service from Dependency Container as a clone, service must implement [`Clone`]
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve<T: Inject + Clone + 'static>(&self) -> Result<T, Error> {
        self.request.resolve::<T>()
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve_shared<T: Inject + 'static>(&self) -> Result<Arc<T>, Error> {
        self.request.resolve_shared::<T>()
    }
    
    /// Inserts the [`Header<T>`] to HTTP request headers
    #[inline]
    pub fn insert_header<T: FromHeaders>(&mut self, header: Header<T>) {
        self.request.insert_header(header)
    }

    /// Executes the request handler for the current HTTP request
    #[inline]
    pub(crate) async fn execute(self) -> HttpResult {
        let (req, pipeline) = self.into_parts();
        if let Some(pipeline) = pipeline {
            pipeline.call(Self::slim(req)).await
        } else { 
            status!(405)
        }
    }
    
    /// Returns a weak reference to global error handler
    #[inline]
    #[cfg(any(feature = "tls", feature = "tracing"))]
    pub(crate) fn error_handler(&self) -> WeakErrorHandler {
        self.request
            .extensions()
            .get::<WeakErrorHandler>()
            .expect("error handler must be provided")
            .clone()
    }

    /// Returns HTTP request parts snapshot
    #[inline]
    #[cfg(any(feature = "tls", feature = "tracing"))]
    pub(crate) fn request_parts_snapshot(&self) -> Arc<Parts> {
        self.request
            .extensions()
            .get::<Arc<Parts>>()
            .expect("http request parts snapshot must be provided")
            .clone()
    }
}

#[cfg(test)]
#[allow(unreachable_pub)]
mod tests {
    use hyper::Request;
    use crate::{HttpBody, headers::custom_headers};
    use super::*;
    
    custom_headers! {
        (Foo, "x-foo")
    }
    
    #[test]
    fn it_debugs() {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        
        let ctx = HttpContext::new(HttpRequest::from_parts(parts, body), None);
        assert_eq!(format!("{ctx:?}"), "HttpContext(..)");
    }
    
    #[test]
    fn it_splits_into_parts() {
        let (parts, body) = Request::get("/test")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let ctx = HttpContext::slim(HttpRequest::from_parts(parts, body));

        let (parts, _) = ctx.into_parts();
        
        assert_eq!(parts.inner.uri(), "/test")
    }

    #[test]
    fn it_inserts_and_header() {
        let (parts, body) = Request::get("/test")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let mut ctx = HttpContext::slim(HttpRequest::from_parts(parts, body));
        ctx.insert_header::<Foo>(Header::from("x-foo"));

        assert_eq!(ctx.extract::<Header<Foo>>().unwrap().into_inner(), "x-foo");
    }
}

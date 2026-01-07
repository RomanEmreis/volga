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

#[cfg(any(
    feature = "tls", 
    feature = "tracing", 
    feature = "di"
))]
use std::sync::Arc;

#[cfg(feature = "rate-limiting")]
use crate::rate_limiting::RateLimiter;

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
    pub fn resolve<T: Send + Sync + Clone + 'static>(&self) -> Result<T, Error> {
        self.request.resolve::<T>()
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve_shared<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, Error> {
        self.request.resolve_shared::<T>()
    }

    /// Returns a reference to a Fixed Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub fn fixed_window_rate_limiter<'a>(&'a self, policy: Option<&'a str>) -> Option<&'a impl RateLimiter> {
        self.request.fixed_window_rate_limiter(policy)
    }

    /// Returns a reference to a Fixed Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub fn sliding_window_rate_limiter<'a>(&'a self, policy: Option<&'a str>) -> Option<&'a impl RateLimiter> {
        self.request.sliding_window_rate_limiter(policy)
    }

    /// Returns iterator of URL path params
    ///
    /// # Example
    /// ```no_run
    /// use volga::middleware::HttpContext;
    ///
    /// # fn docs(ctx: HttpContext) -> std::io::Result<()> {
    /// // https://www.example.com/{key}/{value}
    /// // https://www.example.com/1/test
    /// let mut args = ctx.path_args();
    /// 
    /// assert_eq!(args.next().unwrap(), ("key", "1"));
    /// assert_eq!(args.next().unwrap(), ("value", "test"));
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn path_args(&self) -> impl Iterator<Item = (&str, &str)> {
        self.request.path_args()
    }

    /// Returns iterator of URL query params
    ///
    /// # Example
    /// ```no_run
    /// use volga::middleware::HttpContext;
    ///
    /// # fn docs(ctx: HttpContext) -> std::io::Result<()> {
    /// // https://www.example.com?key=1&value=test
    /// let mut args = ctx.query_args();
    /// 
    /// assert_eq!(args.next().unwrap(), ("key", "1"));
    /// assert_eq!(args.next().unwrap(), ("value", "test"));
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn query_args(&self) -> impl Iterator<Item = (&str, &str)> {
        self.request.query_args()
    }

    /// Inserts the header into the request, replacing any existing values
    /// with the same header name.
    ///
    /// This method always overwrites previous values.
    #[inline]
    pub fn insert_header<T: FromHeaders>(&mut self, header: Header<T>) -> Header<T> {
        self.request.insert_header(header)
    }

    /// Attempts to insert the header into the request, replacing any existing
    /// values with the same header name.
    ///
    /// Returns an error if the header cannot be constructed.
    #[inline]
    pub fn try_insert_header<T>(
        &mut self, 
        header: impl TryInto<Header<T>, Error = Error>
    ) -> Result<Header<T>, Error>
    where
        T: FromHeaders,
    {
        self.request.try_insert_header(header)
    }

    /// Appends a new value for the given header name.
    ///
    /// Existing values with the same name are preserved.
    /// Multiple values for the same header may be present.
    #[inline]
    pub fn append_header<T>(&mut self, header: Header<T>) -> Result<Header<T>, Error>
    where
        T: FromHeaders,
    {
        self.request.append_header(header)
    }

    /// Attempts to append a new value for the given header name.
    ///
    /// Returns an error if the header cannot be constructed or appended.
    #[inline]
    pub fn try_append_header<T>(
        &mut self, 
        header: impl TryInto<Header<T>, Error = Error>
    ) -> Result<Header<T>, Error>
    where
        T: FromHeaders,
    {
        self.request.try_append_header(header)
    }

    /// Removes all values for the given header name.
    ///
    /// Returns `true` if at least one header value was removed.
    #[inline]
    pub fn remove_header<T>(&mut self) -> bool
    where
        T: FromHeaders,
    {
        self.request.remove_header::<T>()
    }

    /// Attempts to remove all values for the given header name.
    ///
    /// Returns `true` if at least one value was removed.
    #[inline]
    pub fn try_remove_header(&mut self, name: &str) -> Result<bool, Error> {
        self.request.try_remove_header(name)
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
    use crate::http::endpoints::route::{PathArg, PathArgs};
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
        
        assert_eq!(parts.uri(), "/test")
    }

    #[test]
    fn it_inserts_and_header() {
        let (parts, body) = Request::get("/test")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let mut ctx = HttpContext::slim(HttpRequest::from_parts(parts, body));
        ctx.insert_header::<Foo>(Header::from_static("x-foo"));

        assert_eq!(ctx.extract::<Header<Foo>>().unwrap().into_inner(), "x-foo");
    }

    #[test]
    fn it_returns_url_path() {
        let args: PathArgs = smallvec::smallvec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ];

        let req = Request::get("/")
            .extension(args)
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let ctx = HttpContext::slim(HttpRequest::from_parts(parts, body));

        let mut args = ctx.path_args();

        assert_eq!(args.next().unwrap(), ("id", "123"));
        assert_eq!(args.next().unwrap(), ("name", "John"));
    }

    #[test]
    fn it_returns_url_query() {
        let req = Request::get("/test?id=123&name=John")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let ctx = HttpContext::slim(HttpRequest::from_parts(parts, body));

        let mut args = ctx.query_args();

        assert_eq!(args.next().unwrap(), ("id", "123"));
        assert_eq!(args.next().unwrap(), ("name", "John"));
    }

    #[test]
    fn it_returns_empty_iter_if_no_path_params() {
        let req = Request::get("/")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let ctx = HttpContext::slim(HttpRequest::from_parts(parts, body));

        let mut args = ctx.path_args();

        assert!(args.next().is_none());
    }

    #[test]
    fn it_returns_empty_iter_if_no_query_params() {
        let req = Request::get("/")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let ctx = HttpContext::slim(HttpRequest::from_parts(parts, body));

        let mut args = ctx.query_args();

        assert!(args.next().is_none());
    }
}

//! Utilities for managing HTTP request scope

use crate::http::endpoints::{
    route::RoutePipeline,
    args::FromRequestRef
};
use crate::{
    error::Error, 
    headers::{Header, HeaderName, HeaderValue, HeaderMap, FromHeaders},
    http::{Method, Uri, Version},
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
    pub(crate) request: HttpRequest,
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

    /// Returns a reference to the associated HTTP method.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpContext, NextFn, http::Method};
    ///
    /// let mut app = App::new();
    ///
    /// app.wrap(|ctx: HttpContext, next: NextFn| async move {
    ///     assert_eq!(*ctx.method(), Method::GET);
    /// });
    /// ```
    #[inline]
    pub fn method(&self) -> &Method {
        self.request.method()
    }

    /// Returns a reference to the associated URI.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpContext, NextFn};
    /// 
    /// let mut app = App::new();
    /// 
    /// app.wrap(|ctx: HttpContext, next: NextFn| async move {
    ///     assert_eq!(req.uri(), "/");
    /// });
    /// ```
    #[inline]
    pub fn uri(&self) -> &Uri {
        self.request.uri()
    }

    /// Represents a version of the HTTP spec.
    #[inline]
    pub fn version(&self) -> Version {
        self.request.version()
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
        self.request.headers_mut().insert(
            header.name(),
            header.value().clone()
        );
        header
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
        let header = header.try_into()?;
        Ok(self.insert_header(header))
    }

    /// Inserts the raw header into the request, replacing any existing values
    /// with the same header name.
    #[inline]
    pub fn insert_raw_header(&mut self, name: HeaderName, value: HeaderValue) {
        self.request.headers_mut().insert(name, value);
    }

    /// Attempts to inserts the raw header into the request, replacing any existing values
    /// with the same header name.
    #[inline]
    pub fn try_insert_raw_header(&mut self, name: &str, value: &str) -> Result<(), Error> {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(Error::from)?;
        let value = HeaderValue::from_str(value)
            .map_err(Error::from)?;

        self.insert_raw_header(name, value);
        Ok(())
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
        self.request.headers_mut().append(
            header.name(),
            header.value().clone()
        );
        Ok(header)
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
        let header = header.try_into()?;
        self.append_header(header)
    }

    /// Appends a new raw value for the given raw header name.
    #[inline]
    pub fn append_raw_header(&mut self, name: HeaderName, value: HeaderValue) {
        self.request.headers_mut().append(name, value);
    }

    /// Attempts to append a new raww value for the given header name.
    #[inline]
    pub fn try_append_raw_header(&mut self, name: &str, value: &str) -> Result<(), Error> {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(Error::from)?;
        let value = HeaderValue::from_str(value)
            .map_err(Error::from)?;

        self.append_raw_header(name, value);
        Ok(())
    }

    /// Removes all values for the given header name.
    ///
    /// Returns `true` if at least one header value was removed.
    #[inline]
    pub fn remove_header<T>(&mut self) -> bool
    where
        T: FromHeaders,
    {
        self.request
            .headers_mut()
            .remove(&T::NAME)
            .is_some()
    }

    /// Attempts to remove all values for the given header name.
    ///
    /// Returns `true` if at least one value was removed.
    #[inline]
    pub fn try_remove_header(&mut self, name: &str) -> Result<bool, Error> {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(Error::from)?;

        Ok(self.request
            .headers_mut()
            .remove(name)
            .is_some())
    }

    /// Returns a typed HTTP header value
    #[inline]
    pub fn get_header<T: FromHeaders>(&self) -> Option<Header<T>> {
        self.request.get_header()
    }

    /// Returns a view of all values associated with this HTTP header.
    #[inline]
    pub fn get_all_headers<T: FromHeaders>(&self) -> impl Iterator<Item = Header<T>> {
        self.request.get_all_headers()
    }

    /// Returns a reference to the associated HTTP header map.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpContext, NextFn};
    ///
    /// let mut app = App::new();
    ///
    /// app.wrap(|ctx: HttpContext, next: NextFn| async move {
    ///     assert!(ctx.headers().is_empty());
    /// });
    /// ```
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        self.request.headers()
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

    fn create_ctx() -> HttpContext {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        HttpContext::slim(HttpRequest::from_parts(parts, body))
    }
    
    #[test]
    fn it_debugs() {
        let ctx = create_ctx();
        assert_eq!(format!("{ctx:?}"), "HttpContext(..)");
    }
    
    #[test]
    fn it_splits_into_parts() {
        let ctx = create_ctx();

        let (parts, _) = ctx.into_parts();
        
        assert_eq!(parts.uri(), "/test")
    }

    #[test]
    fn it_inserts_and_header() {
        let mut ctx = create_ctx();

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
        let ctx = create_ctx();

        let mut args = ctx.path_args();

        assert!(args.next().is_none());
    }

    #[test]
    fn it_returns_empty_iter_if_no_query_params() {
        let ctx = create_ctx();

        let mut args = ctx.query_args();

        assert!(args.next().is_none());
    }

    #[test]
    fn it_inserts_header() {
        let mut ctx = create_ctx();

        let header: Header<Foo> = Header::from_static("some key");
        let _ = ctx.insert_header(header);

        assert_eq!(ctx.headers().get("x-foo").unwrap(), "some key");
    }

    #[test]
    fn it_tries_insert_header() {
        let mut ctx = create_ctx();

        ctx.try_insert_header::<Foo>("some key").unwrap();

        assert_eq!(ctx.get_header::<Foo>().unwrap().value(), "some key");  
    }

    #[test]
    fn it_inserts_raw_header() {
        let mut ctx = create_ctx();

        ctx.insert_raw_header(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("some key"),
        );

        assert_eq!(ctx.headers().get("x-api-key").unwrap(), "some key");
    }

    #[test]
    fn it_tries_insert_raw_header() {
        let mut ctx = create_ctx();

        ctx.try_insert_raw_header("x-foo", "some key").unwrap();

        assert_eq!(ctx.get_header::<Foo>().unwrap().value(), "some key");  
    }

    #[test]
    fn it_appends_header() {
        let mut ctx = create_ctx();

        let api_key_header: Header<Foo> = Header::from_static("1");
        let _ = ctx.append_header(api_key_header);

        let api_key_header: Header<Foo> = Header::from_static("2");
        let _ = ctx.append_header(api_key_header);

        assert_eq!(ctx.headers().get_all("x-foo").into_iter().collect::<Vec<_>>(), ["1", "2"]);
    }

    #[test]
    fn it_tries_append_header() {
        let mut ctx = create_ctx();

        ctx.try_append_header::<Foo>("1").unwrap();
        ctx.try_append_header::<Foo>("2").unwrap();

        assert_eq!(ctx.get_all_headers::<Foo>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]);  
    }

    #[test]
    fn it_appends_raw_header() {
        let mut ctx = create_ctx();

        ctx.append_raw_header(
            HeaderName::from_static("x-foo"),
            HeaderValue::from_static("1"),
        );

        ctx.append_raw_header(
            HeaderName::from_static("x-foo"),
            HeaderValue::from_static("2"),
        );

        assert_eq!(ctx.get_all_headers::<Foo>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]); 
    }

    #[test]
    fn it_tries_appends_raw_header() {
        let mut ctx = create_ctx();

        ctx.try_append_raw_header("x-api-key", "1").unwrap();
        ctx.try_append_raw_header("x-api-key", "2").unwrap();

        assert_eq!(ctx.get_all_headers::<Foo>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]); 
    }

    #[test]
    fn it_removes_header() {
        let mut ctx = create_ctx();

        let header: Header<Foo> = Header::from_static("some key");
        let _ = ctx.insert_header(header);

        ctx.remove_header::<Foo>();

        assert!(ctx.headers().get("x-foo").is_none());
    }

    #[test]
    fn it_tries_remove_header() {
        let mut ctx = create_ctx();

        let api_key_header: Header<Foo> = Header::from_static("some key");
        let _ = ctx.insert_header(api_key_header);

        let result = ctx.try_remove_header("x-foo").unwrap();

        assert!(result);
        assert!(ctx.headers().get("x-api-key").is_none());
    }
}

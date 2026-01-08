//! HTTP request utilities

use http_body_util::BodyDataStream;
use hyper::{
    body::Incoming,
};

use crate::{
    error::Error,
    headers::{FromHeaders, Header, HeaderName},
    HttpBody,
    UnsyncBoxBody,
    BoxBody
};

use crate::http::{
    endpoints::{args::FromRequestRef, route::PathArgs}, 
    request::request_body_limit::RequestBodyLimit,
    Request,
    Parts,
    Extensions,
    Method,
    Uri,
    Version
};

#[cfg(feature = "rate-limiting")]
use crate::rate_limiting::{
    GlobalRateLimiter,
    RateLimiter
};

#[cfg(feature = "di")]
use crate::di::Container;
#[cfg(any(feature = "di", feature = "rate-limiting"))]
use std::sync::Arc;
use crate::headers::HeaderMap;

pub mod request_body_limit;

/// Wraps the incoming [`Request`] to enrich its functionality
pub struct HttpRequest {
    /// Inner [`Request`]
    inner: Request<HttpBody>
}

impl std::fmt::Debug for HttpRequest {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpRequest(..)")
    }
}

impl HttpRequest {
    /// Creates a new [`HttpRequest`]
    pub(crate) fn new(request: Request<Incoming>) -> Self {
        Self { inner: request.map(HttpBody::incoming) }
    }

    /// Returns a reference to the associated URI.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest};
    /// 
    /// let mut app = App::new();
    /// 
    /// app.map_get("/", |req: HttpRequest| async move {
    ///     assert_eq!(req.uri(), "/");
    /// });
    /// ```
    #[inline]
    pub fn uri(&self) -> &Uri {
        self.inner.uri()
    }
    
    /// Returns a reference to the associated HTTP header map.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest};
    ///
    /// let mut app = App::new();
    ///
    /// app.map_get("/", |req: HttpRequest| async move {
    ///     assert!(req.headers().is_empty());
    /// });
    /// ```
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    /// Returns a mutable reference to the associated extensions.
    #[inline]
    #[allow(unused)]
    pub(crate) fn headers_mut(&mut self) -> &mut HeaderMap {
        self.inner.headers_mut()
    }
    
    /// Returns a reference to the associated HTTP method.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest, http::Method};
    ///
    /// let mut app = App::new();
    ///
    /// app.map_get("/", |req: HttpRequest| async move {
    ///     assert_eq!(*req.method(), Method::GET);
    /// });
    /// ```
    #[inline]
    pub fn method(&self) -> &Method {
        self.inner.method()
    }

    /// Represents a version of the HTTP spec.
    #[inline]
    pub fn version(&self) -> Version {
        self.inner.version()
    }
    
    /// Returns a reference to the associated extensions.
    #[inline]
    pub(crate) fn extensions(&self) -> &Extensions {
        self.inner.extensions()
    }

    /// Returns a mutable reference to the associated extensions.
    #[inline]
    #[cfg(any(feature = "tls", feature = "tracing", feature = "auth"))]
    pub(crate) fn extensions_mut(&mut self) -> &mut Extensions {
        self.inner.extensions_mut()
    }

    /// Returns this [`HttpRequest`] body limit.
    pub fn body_limit(&self) -> Option<usize> {
        self.inner.extensions()
            .get::<RequestBodyLimit>()
            .and_then(|l| match l {
                RequestBodyLimit::Enabled(size) => Some(*size),
                RequestBodyLimit::Disabled => None,
            })
    }

    #[inline]
    pub(crate) fn into_limited(self, body_limit: RequestBodyLimit) -> Self {
        match body_limit {
            RequestBodyLimit::Disabled => self,
            RequestBodyLimit::Enabled(limit) => {
                let (parts, body) = self.into_parts();
                let body = HttpBody::limited(body, limit);
                Self::from_parts(parts, body)
            }
        }
    }

    /// Consumes the request and returns just the body
    #[inline]
    pub fn into_body(self) -> HttpBody {
        self.inner.into_body()
    }

    /// Consumes the request and returns the body as a boxed trait object
    #[inline]
    pub fn into_boxed_body(self) -> BoxBody {
        self.inner
            .into_body()
            .into_boxed()
    }

    /// Consumes the request body into [`BodyDataStream`]
    #[inline]
    pub fn into_body_stream(self) -> BodyDataStream<HttpBody> {
        self.inner
            .into_body()
            .into_data_stream()
    }

    /// Consumes the request and returns the body as a boxed trait object that is !Sync
    #[inline]
    pub fn into_boxed_unsync_body(self) -> UnsyncBoxBody {
        self.inner
            .into_body()
            .into_boxed_unsync()
    }
    
    /// Consumes the request and returns request head and body
    pub(crate) fn into_parts(self) -> (Parts, HttpBody) {
        self.inner.into_parts()
    }

    /// Creates a new `HttpRequest` with the given head and body
    pub(crate) fn from_parts(parts: Parts, body: HttpBody) -> Self {
        let request = Request::from_parts(parts, body);
        Self { inner: request }
    }

    /// Creates a new `HttpRequest` with the given head and empty body
    pub(crate) fn slim(parts: &Parts) -> Self {
        let request = Request::from_parts(parts.clone(), HttpBody::empty());
        Self { inner: request }
    }

    /// Returns a reference to a Fixed Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub fn fixed_window_rate_limiter(&self, policy: Option<&str>) -> Option<&impl RateLimiter> {
        self.inner.extensions()
            .get::<Arc<GlobalRateLimiter>>()?
            .fixed_window(policy)
    }

    /// Returns a reference to a Sliding Window Rate Limiter
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub fn sliding_window_rate_limiter(&self, policy: Option<&str>) -> Option<&impl RateLimiter> {
        self.inner.extensions()
            .get::<Arc<GlobalRateLimiter>>()?
            .sliding_window(policy)
    }
    
    /// Returns a reference to the DI container of the request scope
    #[inline]
    #[cfg(feature = "di")]
    pub fn container(&self) -> &Container {
        self.inner.extensions()
            .get::<Container>()
            .expect("DI Container must be provided")
    }

    /// Resolves a service from Dependency Container as a clone, service must implement [`Clone`]
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve<T: Send + Sync + Clone + 'static>(&self) -> Result<T, Error> {
        self.container()
            .resolve::<T>()
            .map_err(Into::into)
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve_shared<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, Error> {
        self.container()
            .resolve_shared::<T>()
            .map_err(Into::into)
    }
    
    /// Extracts a payload from request parts
    ///
    /// # Example
    /// ```no_run
    /// use volga::{HttpRequest, Query};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Params {
    ///     key: u32,
    ///     value: String
    /// }
    ///
    /// # fn docs(req: HttpRequest) -> std::io::Result<()> {
    /// // https://www.example.com?key=1&value=test
    /// let params: Query<Params> = req.extract()?;
    /// 
    /// assert_eq!(params.key, 1u32);
    /// assert_eq!(params.value, "test");
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn extract<T: FromRequestRef>(&self) -> Result<T, Error> {
        T::from_request(self)
    }

    /// Returns iterator of URL path params
    ///
    /// # Example
    /// ```no_run
    /// use volga::HttpRequest;
    ///
    /// # fn docs(req: HttpRequest) -> std::io::Result<()> {
    /// // https://www.example.com/{key}/{value}
    /// // https://www.example.com/1/test
    /// let mut args = req.path_args();
    /// 
    /// assert_eq!(args.next().unwrap(), ("key", "1"));
    /// assert_eq!(args.next().unwrap(), ("value", "test"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn path_args(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner.extensions()
            .get::<PathArgs>()
            .map(|args| args
                .iter()
                .map(|arg| (arg.name.as_ref(), arg.value.as_ref())))
            .into_iter()
            .flatten()
    }

    /// Returns iterator of URL query params
    ///
    /// # Example
    /// ```no_run
    /// use volga::HttpRequest;
    ///
    /// # fn docs(req: HttpRequest) -> std::io::Result<()> {
    /// // https://www.example.com?key=1&value=test
    /// let mut args = req.query_args();
    /// 
    /// assert_eq!(args.next().unwrap(), ("key", "1"));
    /// assert_eq!(args.next().unwrap(), ("value", "test"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn query_args(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner.uri()
            .query()
            .map(|query| query.split('&')
                .map(|arg| {
                    let mut parts = arg.split('=');
                    let key = parts.next().unwrap();
                    let value = parts.next().unwrap();
                    (key, value)
                }))
            .into_iter()
            .flatten()
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

    /// Inserts the header into the request, replacing any existing values
    /// with the same header name.
    ///
    /// This method always overwrites previous values.
    #[inline]
    pub fn insert_header<T>(&mut self, header: Header<T>) -> Header<T>
    where
        T: FromHeaders,
    {
        self.inner.headers_mut().insert(
            header.name(),
            header.value().clone()
        );
        header
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
        self.inner.headers_mut().append(
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

    /// Removes all values for the given header name.
    ///
    /// Returns `true` if at least one header value was removed.
    #[inline]
    pub fn remove_header<T>(&mut self) -> bool
    where
        T: FromHeaders,
    {
        self.inner
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

        Ok(self.inner.headers_mut().remove(name).is_some())
    }
}

#[cfg(test)]
#[allow(unreachable_pub)]
mod tests {
    use http_body_util::BodyExt;
    use crate::headers::{Header, Vary, custom_headers};
    use crate::http::endpoints::route::PathArg;
    use super::*;
    
    #[cfg(feature = "di")]
    use std::collections::HashMap;
    #[cfg(feature = "di")]
    use std::sync::Mutex;
    
    #[cfg(feature = "di")]
    use crate::di::ContainerBuilder;

    custom_headers! {
        (Foo, "x-foo")
    }
    
    #[cfg(feature = "di")]
    #[allow(dead_code)]
    #[derive(Clone, Default)]
    struct InMemoryCache {
        inner: Arc<Mutex<HashMap<String, String>>>
    }
    
    #[test]
    fn it_inserts_header() {
        let req = Request::get("http://localhost")
            .body(HttpBody::empty())
            .unwrap();
        
        let (parts, body) = req.into_parts();
        let mut http_req = HttpRequest::from_parts(parts, body);
        let header = Header::<Vary>::from_static("foo");
        
        http_req.insert_header(header);
        
        assert_eq!(http_req.headers().get("vary").unwrap(), "foo");
    }
    
    #[test]
    fn it_extracts_from_request_ref() {
        let req = Request::get("http://localhost/")
            .header("vary", "foo")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        
        let header = http_req.extract::<Header<Vary>>().unwrap();
        
        assert_eq!(header.value(), "foo");
    }
    
    #[tokio::test]
    async fn it_unwraps_body() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        
        let body = http_req
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();

        assert_eq!(String::from_utf8_lossy(&body), "foo");
    }

    #[tokio::test]
    async fn it_unwraps_inner_req() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let body = http_req
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();

        assert_eq!(String::from_utf8_lossy(&body), "foo");
    }
    
    #[test]
    #[cfg(feature = "di")]
    #[should_panic]
    fn it_panic_if_there_is_no_di_container() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        
        _ = http_req.container();
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

        let cache = http_req.resolve::<InMemoryCache>();
        
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

        let cache = http_req.resolve_shared::<InMemoryCache>();

        assert!(cache.is_ok());
    }

    #[test]
    fn it_debugs() {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let req = HttpRequest::from_parts(parts, body);
        assert_eq!(format!("{req:?}"), "HttpRequest(..)");
    }

    #[test]
    fn it_splits_into_parts() {
        let (parts, body) = Request::get("/test")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let ctx = HttpRequest::from_parts(parts, body);

        let (parts, _) = ctx.into_parts();

        assert_eq!(parts.uri, "/test")
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
        let req = HttpRequest::from_parts(parts, body);

        let mut args = req.path_args();

        assert_eq!(args.next().unwrap(), ("id", "123"));
        assert_eq!(args.next().unwrap(), ("name", "John"));
    }

    #[test]
    fn it_returns_url_query() {
        let req = Request::get("/test?id=123&name=John")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let mut args = req.query_args();

        assert_eq!(args.next().unwrap(), ("id", "123"));
        assert_eq!(args.next().unwrap(), ("name", "John"));
    }

    #[test]
    fn it_returns_empty_iter_if_no_path_params() {
        let req = Request::get("/")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let mut args = req.path_args();

        assert!(args.next().is_none());
    }

    #[test]
    fn it_returns_empty_iter_if_no_query_params() {
        let req = Request::get("/")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let mut args = req.query_args();

        assert!(args.next().is_none());
    }

    #[test]
    fn it_inserts_and_header() {
        let (parts, body) = Request::get("/test")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let mut req = HttpRequest::from_parts(parts, body);
        req.insert_header::<Foo>(Header::from_static("x-foo"));

        assert_eq!(req.extract::<Header<Foo>>().unwrap().into_inner(), "x-foo");
    }

    #[test]
    fn it_gets_body_limit() {
        let (parts, body) = Request::get("/test")
            .extension(RequestBodyLimit::Enabled(100))
            .body(HttpBody::full("Hello, World!"))
            .unwrap()
            .into_parts();

        let req = HttpRequest::from_parts(parts, body);

        assert_eq!(req.body_limit(), Some(100))
    }
}

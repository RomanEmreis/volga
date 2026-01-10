//! HTTP request utilities

use hyper::{
    body::Incoming,
};

use crate::{
    error::Error,
    headers::{FromHeaders, Header},
    HttpBody,
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

use crate::headers::HeaderMap;

#[cfg(feature = "middleware")]
pub use request_mut::{HttpRequestMut, IntoTapResult};

#[cfg(feature = "middleware")]
mod request_mut;
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
    
    /// Consumes the request and returns request head and body
    pub(crate) fn into_parts(self) -> (Parts, HttpBody) {
        self.inner.into_parts()
    }

    /// Creates a new `HttpRequest` with the given head and body
    pub(crate) fn from_parts(parts: Parts, body: HttpBody) -> Self {
        let request = Request::from_parts(parts, body);
        Self { inner: request }
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

    /// Returns a typed HTTP header value
    #[inline]
    pub fn get_header<T: FromHeaders>(&self) -> Option<Header<T>> {
        self.headers()
            .get(T::NAME)
            .map(Header::new)
    }

    /// Returns a view of all values associated with this HTTP header.
    #[inline]
    pub fn get_all_headers<T: FromHeaders>(&self) -> impl Iterator<Item = Header<T>> {
        self.headers()
            .get_all(T::NAME)
            .iter()
            .map(Header::new)
    }
}

#[cfg(test)]
#[allow(unreachable_pub)]
mod tests {
    use http_body_util::BodyExt;
    use crate::headers::{Header, Vary, headers};
    use crate::http::endpoints::route::PathArg;
    use super::*;

    headers! {
        (Foo, "x-foo")
    }
    
    #[test]
    fn it_inserts_header() {
        let req = Request::get("http://localhost")
            .body(HttpBody::empty())
            .unwrap();
        
        let (parts, body) = req.into_parts();
        let mut http_req = HttpRequest::from_parts(parts, body);
        
        http_req.headers_mut().insert("vary", "foo".parse().unwrap());
        
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
    fn it_gets_header() {
        let (parts, body) = Request::get("/test")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let mut req = HttpRequest::from_parts(parts, body);
        req.headers_mut().insert("x-foo", "val".parse().unwrap());

        assert_eq!(req.get_header::<Foo>().unwrap().value(), "val");
    }

    #[test]
    fn it_gets_many_headers() {
        let (parts, body) = Request::get("/test")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let mut req = HttpRequest::from_parts(parts, body);
        req.headers_mut().append("x-foo", "1".parse().unwrap());
        req.headers_mut().append("x-foo", "2".parse().unwrap());

        assert_eq!(req.get_all_headers::<Foo>().map(|h| h.value().clone()).collect::<Vec<_>>(), ["1", "2"]);
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

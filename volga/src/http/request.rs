//! HTTP request utilities

use crate::{
    HttpBody,
    error::Error,
    headers::{FromHeaders, Header},
};
use hyper::body::Incoming;
use query_args::{QueryArgsCache, QueryArgsIter};
use std::sync::OnceLock;

use crate::http::{
    Extensions, Method, Parts, Request, Uri, Version,
    endpoints::args::{FromRequestRef, PathArg},
    request::request_body_limit::RequestBodyLimit,
    request_scope::HttpRequestScope,
};

use crate::headers::HeaderMap;

#[cfg(feature = "middleware")]
pub use request_mut::{HttpRequestMut, IntoTapResult};

mod query_args;
pub mod request_body_limit;
#[cfg(feature = "middleware")]
mod request_mut;

/// Wraps the incoming [`Request`] to enrich its functionality
pub struct HttpRequest {
    /// The head of the request, apart from the body, as a [`Request`] does not lend it: the
    /// extractors of a handler read it here, without taking the request to pieces
    parts: Parts,
    body: HttpBody,
    /// Boxed, as almost no request fills it, and the request moves by value through the
    /// pipeline: inline, it was 160 of the 432 bytes every handler's future holds and copies
    query_args_cache: OnceLock<Box<QueryArgsCache>>,
}

impl std::fmt::Debug for HttpRequest {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpRequest(..)")
    }
}

impl HttpRequest {
    /// Creates a new [`HttpRequest`]
    #[inline]
    pub(crate) fn new(parts: Parts, body: Incoming) -> Self {
        Self::from_parts(parts, HttpBody::incoming(body))
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
        &self.parts.uri
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
        &self.parts.headers
    }

    /// Returns a mutable reference to the associated HTTP header map.
    #[inline]
    #[allow(unused)]
    pub(crate) fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.parts.headers
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
        &self.parts.method
    }

    /// Represents a version of the HTTP spec.
    #[inline]
    pub fn version(&self) -> Version {
        self.parts.version
    }

    /// Returns a reference to the associated extensions.
    #[inline]
    pub(crate) fn extensions(&self) -> &Extensions {
        &self.parts.extensions
    }

    /// Returns a mutable reference to the associated extensions.
    #[inline]
    #[allow(unused)]
    pub(crate) fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.parts.extensions
    }

    /// Returns the body limit of this [`HttpRequest`], in bytes, or `None` when it has none.
    ///
    /// It is the limit of the route the request was routed to, if it set one, or else that
    /// of its route group, or else the application's - see
    /// [`Route::with_body_limit`](crate::routing::Route::with_body_limit).
    pub fn body_limit(&self) -> Option<usize> {
        match self.parts.extensions.get::<HttpRequestScope>()?.body_limit {
            RequestBodyLimit::Enabled(size) => Some(size),
            RequestBodyLimit::Disabled => None,
        }
    }

    #[inline]
    pub(crate) fn into_limited(mut self, body_limit: RequestBodyLimit) -> Self {
        if let RequestBodyLimit::Enabled(limit) = body_limit {
            self.body = HttpBody::limited(self.body, limit);
        }
        self
    }

    /// Consumes the request and returns just the body
    #[inline]
    pub fn into_body(self) -> HttpBody {
        self.body
    }

    /// Returns a reference to the head of the request
    #[inline]
    pub(crate) fn parts(&self) -> &Parts {
        &self.parts
    }

    /// Takes the body, leaving an empty one behind
    #[inline]
    pub(crate) fn take_body(&mut self) -> HttpBody {
        std::mem::replace(&mut self.body, HttpBody::empty())
    }

    /// Puts `body` in place of the body of the request, under the body limit of the request,
    /// as the body it replaces was
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn set_body(&mut self, body: HttpBody) {
        self.body = match self.body_limit() {
            Some(limit) => HttpBody::limited(body, limit),
            None => body,
        };
    }

    /// Takes the request, leaving an empty one behind
    #[inline]
    pub(crate) fn take(&mut self) -> Self {
        let (parts, ()) = Request::new(()).into_parts();
        std::mem::replace(self, Self::from_parts(parts, HttpBody::empty()))
    }

    /// Returns the path parameter at `index` by position, for a handler argument to read
    #[inline]
    pub(crate) fn path_arg_mut(&mut self, index: usize) -> Option<&mut PathArg> {
        self.parts
            .extensions
            .get_mut::<HttpRequestScope>()?
            .params
            .get_mut(index)
    }

    /// Consumes the request and returns request head and body, for the deprecated
    /// `HttpRequestMut::into_parts` alone, and goes with it
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn into_parts(self) -> (Parts, HttpBody) {
        (self.parts, self.body)
    }

    /// Creates a new `HttpRequest` with the given head and body
    #[inline]
    pub(crate) fn from_parts(parts: Parts, body: HttpBody) -> Self {
        Self {
            parts,
            body,
            query_args_cache: OnceLock::new(),
        }
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
        self.parts
            .extensions
            .get::<HttpRequestScope>()
            .map(|s| &s.params)
            .into_iter()
            .flat_map(|args| {
                args.iter()
                    .map(|arg| (arg.name.as_ref(), arg.value.as_ref()))
            })
    }

    /// Returns iterator of URL query params
    ///
    /// > Note: Only `key=value` pairs are yielded. Arguments without `=` are ignored.
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
        let query = self.parts.uri.query().unwrap_or_default();
        let cache = self
            .query_args_cache
            .get_or_init(|| Box::new(QueryArgsCache::new(query)));
        QueryArgsIter::new(query, cache)
    }

    /// Returns the value of the first query parameter named `name`, by the rules of
    /// [`query_args`](Self::query_args), without building its cache: for a lookup of one key
    /// per request, which then allocates nothing
    #[inline]
    #[cfg(feature = "rate-limiting")]
    pub(crate) fn query_arg(&self, name: &str) -> Option<&str> {
        query_args::find(self.parts.uri.query().unwrap_or_default(), name)
    }

    /// Returns a typed HTTP header value
    #[inline]
    pub fn get_header<T: FromHeaders>(&self) -> Option<Header<T>> {
        self.headers().get(T::NAME).map(Header::from_ref)
    }

    /// Returns a view of all values associated with this HTTP header.
    #[inline]
    pub fn get_all_headers<T: FromHeaders>(&self) -> impl Iterator<Item = Header<T>> {
        self.headers().get_all(T::NAME).iter().map(Header::from_ref)
    }
}

#[cfg(test)]
#[allow(unreachable_pub)]
#[allow(unused)]
mod tests {
    use super::*;
    use crate::headers::{Header, Vary, headers};
    use crate::http::endpoints::route::PathArg;
    use http_body_util::BodyExt;

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

        http_req
            .headers_mut()
            .insert("vary", "foo".parse().unwrap());

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

        let body = http_req.into_body().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(&body), "foo");
    }

    #[tokio::test]
    async fn it_unwraps_inner_req() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let body = http_req.into_body().collect().await.unwrap().to_bytes();

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
    #[cfg(feature = "middleware")]
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
        use crate::http::endpoints::route::{PathArg, PathArgs};
        use crate::http::request_scope::HttpRequestScope;
        let args: PathArgs = smallvec::smallvec![
            PathArg {
                name: "id".into(),
                value: "123".into()
            },
            PathArg {
                name: "name".into(),
                value: "John".into()
            }
        ]
        .into();

        let req = Request::get("/")
            .extension(HttpRequestScope {
                params: args,
                ..HttpRequestScope::default()
            })
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let mut args = req.path_args();
        let mut args_2 = req.path_args();

        assert_eq!(Vec::from_iter(args), Vec::from_iter(args_2));

        let mut args = req.path_args();

        assert_eq!(args.next().unwrap(), ("id", "123"));
        assert_eq!(args.next().unwrap(), ("name", "John"));
    }

    #[test]
    fn it_returns_url_query() {
        let req = Request::get("/test?id=123&name=John&age")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let mut args = req.query_args();

        assert_eq!(args.next().unwrap(), ("id", "123"));
        assert_eq!(args.next().unwrap(), ("name", "John"));
        assert!(args.next().is_none());
    }

    #[test]
    fn it_builds_the_query_cache_when_query_args_is_first_called() {
        let (parts, body) = Request::get("/test?id=123")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        let req = HttpRequest::from_parts(parts, body);

        assert!(req.query_args_cache.get().is_none());
        assert_eq!(req.query_args().collect::<Vec<_>>(), [("id", "123")]);
        assert!(req.query_args_cache.get().is_some());
        assert_eq!(req.query_args().collect::<Vec<_>>(), [("id", "123")]);
    }

    #[test]
    #[cfg(feature = "rate-limiting")]
    fn it_looks_up_a_query_arg_without_building_the_cache() {
        let (parts, body) = Request::get("/test?id=123&name=John&id=7")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        let req = HttpRequest::from_parts(parts, body);

        assert_eq!(req.query_arg("id"), Some("123"));
        assert_eq!(req.query_arg("name"), Some("John"));
        assert_eq!(req.query_arg("age"), None);
        assert!(req.query_args_cache.get().is_none());
    }

    #[test]
    fn it_returns_empty_iter_if_no_path_params() {
        let req = Request::get("/").body(HttpBody::empty()).unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let mut args = req.path_args();

        assert!(args.next().is_none());
    }

    #[test]
    fn it_returns_empty_iter_if_no_query_params() {
        let req = Request::get("/").body(HttpBody::empty()).unwrap();

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

        assert_eq!(
            req.get_all_headers::<Foo>()
                .map(|h| h.value().clone())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
    }

    #[test]
    fn it_gets_body_limit() {
        use crate::http::request_scope::HttpRequestScope;
        let scope = HttpRequestScope {
            body_limit: RequestBodyLimit::Enabled(100),
            ..HttpRequestScope::default()
        };
        let (parts, body) = Request::get("/test")
            .extension(scope)
            .body(HttpBody::full("Hello, World!"))
            .unwrap()
            .into_parts();

        let req = HttpRequest::from_parts(parts, body);

        assert_eq!(req.body_limit(), Some(100))
    }
}

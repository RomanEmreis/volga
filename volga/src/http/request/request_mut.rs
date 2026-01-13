//! Mutable version of [`HttpRequest`] for middleware

use crate::{
    headers::{FromHeaders, HeaderMap, Header, HeaderName, HeaderValue}, 
    http::{Parts, Extensions, Method, Uri, Version, FromRequestRef}, 
    error::Error, 
    HttpRequest, 
    HttpBody
};

/// A mutable HTTP request used during the middleware pipeline.
///
/// `HttpRequestMut` represents the *mutable request phase*.
/// It is available only while the request is being processed by
/// middleware (`wrap`, `tap_req`, etc.).
///
/// ## Phase model
///
/// - Middleware phase: `HttpRequestMut`
/// - Handler phase: [`HttpRequest`]
///
/// During this phase the request **metadata may be modified**
/// (headers, extensions, limits), but the request body
/// **cannot be consumed**.
///
/// To transition into the handler phase, call [`freeze`],
/// which produces an immutable [`HttpRequest`].
///
/// ## Design notes
///
/// - `HttpRequestMut` owns the underlying request
/// - It intentionally does **not** implement `Deref`
/// - Methods that would consume the body are unavailable by design
///
/// This type exists to enforce the request lifecycle at the type level.
pub struct HttpRequestMut {
    inner: HttpRequest,
}

impl std::fmt::Debug for HttpRequestMut {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpRequestMut(..)")
    }
}

impl HttpRequestMut {
    /// Creates a new [`HttpRequestMut`] from [`HttpRequest`]
    #[inline]
    pub(crate) fn new(inner: HttpRequest) -> Self {
        Self { inner }
    }
    
    /// Returns a reference to the associated URI.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequestMut};
    ///
    /// let mut app = App::new();
    ///
    /// app.tap_req(|req: HttpRequestMut| async move {
    ///     assert_eq!(req.uri(), "/");
    ///     req
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
    /// use volga::{App, HttpRequestMut};
    ///
    /// let mut app = App::new();
    ///
    /// app.tap_req(|req: HttpRequestMut| async move {
    ///     assert!(req.headers().is_empty());
    ///     req
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
    /// use volga::{App, HttpRequestMut, http::Method};
    ///
    /// let mut app = App::new();
    ///
    /// app.tap_req(|req: HttpRequestMut| async move {
    ///     assert_eq!(*req.method(), Method::GET);
    ///     req
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
    #[allow(unused)]
    pub(crate) fn extensions(&self) -> &Extensions {
        self.inner.extensions()
    }

    /// Returns a mutable reference to the associated extensions.
    #[inline]
    #[allow(unused)]
    pub(crate) fn extensions_mut(&mut self) -> &mut Extensions {
        self.inner.extensions_mut()
    }
    
    /// Returns a typed HTTP header value
    #[inline]
    pub fn get_header<T: FromHeaders>(&self) -> Option<Header<T>> {
        self.inner.get_header()
    }

    /// Returns a view of all values associated with this HTTP header.
    #[inline]
    pub fn get_all_headers<T: FromHeaders>(&self) -> impl Iterator<Item = Header<T>> {
        self.inner.get_all_headers()
    }
    
    /// Inserts the header into the request, replacing any existing values
    /// with the same header name.
    ///
    /// This method always overwrites previous values.
    #[inline]
    pub fn insert_header<T: FromHeaders>(&mut self, header: Header<T>) -> Header<T> {
        self.inner.headers_mut().insert(
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
        self.inner.headers_mut().insert(name, value);
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

    /// Appends a new raw value for the given raw header name.
    #[inline]
    pub fn append_raw_header(&mut self, name: HeaderName, value: HeaderValue) {
        self.inner.headers_mut().append(name, value);
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

        Ok(self.inner
            .headers_mut()
            .remove(name)
            .is_some())
    }

    /// Returns this [`HttpRequest`] body limit.
    pub fn body_limit(&self) -> Option<usize> {
        self.inner.body_limit()
    }

    /// Extracts a payload from request parts
    ///
    /// # Example
    /// ```no_run
    /// use volga::{HttpRequestMut, Query};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Params {
    ///     id: u32,
    ///     key: String
    /// }
    ///
    /// # fn docs(req: HttpRequestMut) -> std::io::Result<()> {
    /// let params: Query<Params> = req.extract()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn extract<T: FromRequestRef>(&self) -> Result<T, Error> {
        self.inner.extract()
    }

    /// Returns iterator of URL path params
    ///
    /// # Example
    /// ```no_run
    /// use volga::HttpRequestMut;
    ///
    /// # fn docs(req: HttpRequestMut) -> std::io::Result<()> {
    /// // https://www.example.com/{key}/{value}
    /// // https://www.example.com/1/test
    /// let mut args = req.path_args();
    ///
    /// assert_eq!(args.next().unwrap(), ("key", "1"));
    /// assert_eq!(args.next().unwrap(), ("value", "test"));
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn path_args(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner.path_args()
    }

    /// Returns iterator of URL query params
    /// 
    /// > Note: Only `key=value` pairs are yielded. Arguments without `=` are ignored.
    ///
    /// # Example
    /// ```no_run
    /// use volga::HttpRequestMut;
    ///
    /// # fn docs(req: HttpRequestMut) -> std::io::Result<()> {
    /// // https://www.example.com?key=1&value=test
    /// let mut args = req.query_args();
    ///
    /// assert_eq!(args.next().unwrap(), ("key", "1"));
    /// assert_eq!(args.next().unwrap(), ("value", "test"));
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn query_args(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner.query_args()
    }

    /// Returns a read-only view of the request.
    ///
    /// This view does not allow mutation or body consumption
    /// and is intended for request inspection and extraction.
    pub fn as_read_only(&self) -> &HttpRequest {
        &self.inner
    }
    
    /// Transitions the request into the immutable handler phase.
    ///
    /// This method consumes `HttpRequestMut` and returns an
    /// immutable [`HttpRequest`].
    ///
    /// After calling `freeze`:
    /// - request metadata can no longer be modified
    /// - the request body may be consumed
    /// - the request can be passed to a handler
    ///
    /// This transition is **explicit and irreversible** by design.
    #[inline]
    pub(crate) fn freeze(self) -> HttpRequest {
        self.inner
    }

    /// Consumes the request and returns request head and body
    #[inline]
    pub fn into_parts(self) -> (Parts, HttpBody) {
        self.inner.into_parts()
    }

    /// Creates a new `HttpRequest` with the given head and body
    #[inline]
    pub fn from_parts(parts: Parts, body: HttpBody) -> Self {
        let inner = HttpRequest::from_parts(parts, body);
        Self { inner }
    }
}

/// Conversion trait for values returned from [`tap_req`] middleware.
///
/// This trait exists solely to make `tap_req` middleware ergonomic:
/// a middleware may either return the request directly or fail with an error.
///
/// # Supported return types
///
/// * [`HttpRequestMut`] — returned as-is
/// * `Result<HttpRequestMut, Error>` — propagated without modification
///
/// # Examples
///
/// Returning the request directly:
///
/// ```no_run
/// use volga::{App, HttpRequestMut};
///
/// let mut app = App::new();
/// 
/// app.tap_req(|req: HttpRequestMut| async move {
///     println!("{:?}", req.headers().get("x-header"));
///     req
/// });
/// ```
///
/// Returning a fallible result:
///
/// ```no_run
/// use volga::{App, HttpRequestMut, error::Error};
/// # use volga::HttpBody;
/// 
/// let mut app = App::new();
/// 
/// app.tap_req(|req: HttpRequestMut| async move {
///     let (parts, body) = req.into_parts();
///     let body = exotic_decompression(body)?;
///
///     Ok(HttpRequestMut::from_parts(parts, body))
/// });
/// 
/// # fn exotic_decompression(body: HttpBody) -> Result<HttpBody, Error> {
/// #     Ok(body)
/// # }
/// ```
///
/// # Notes
///
/// This trait is **not intended to be implemented by users**.
/// Only framework-provided implementations are supported.
pub trait IntoTapResult : sealed::Sealed {
    /// Converts the value into a `Result<HttpRequestMut, Error>`.
    fn into_result(self) -> Result<HttpRequestMut, Error>;
}

impl IntoTapResult for HttpRequestMut {
    #[inline]
    fn into_result(self) -> Result<HttpRequestMut, Error> {
        Ok(self)
    }
}

impl IntoTapResult for Result<HttpRequestMut, Error> {
    #[inline]
    fn into_result(self) -> Result<HttpRequestMut, Error> {
        self
    }
}

mod sealed {
    use crate::{HttpRequestMut, error::Error};

    pub trait Sealed {}
    
    impl Sealed for HttpRequestMut {}
    impl Sealed for Result<HttpRequestMut, Error> {}
}

#[cfg(test)]
#[allow(unreachable_pub)]
mod tests {
    use super::*;
    use crate::headers::headers;
    use crate::http::Request;
    use crate::HttpBody;
    use crate::http::endpoints::route::{PathArg, PathArgs};

    headers! {
        (Foo, "x-foo")
    }

    fn create_req() -> HttpRequestMut {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        HttpRequestMut::new(HttpRequest::from_parts(parts, body))
    }

    #[test]
    fn it_debugs() {
        let req = create_req();
        assert_eq!(format!("{req:?}"), "HttpRequestMut(..)");
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
        let req = HttpRequestMut::new(HttpRequest::from_parts(parts, body));

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
        let req = HttpRequestMut::new(HttpRequest::from_parts(parts, body));

        let mut args = req.query_args();

        assert_eq!(args.next().unwrap(), ("id", "123"));
        assert_eq!(args.next().unwrap(), ("name", "John"));
    }

    #[test]
    fn it_returns_empty_iter_if_no_path_params() {
        let req = create_req();

        let mut args = req.path_args();

        assert!(args.next().is_none());
    }

    #[test]
    fn it_returns_empty_iter_if_no_query_params() {
        let req = create_req();

        let mut args = req.query_args();

        assert!(args.next().is_none());
    }
    
    #[test]
    fn it_inserts_header() {
        let mut req = create_req();

        let header: Header<Foo> = Header::from_static("some key");
        let _ = req.insert_header(header);

        assert_eq!(req.headers().get("x-foo").unwrap(), "some key");
    }
    
    #[test]
    fn it_tries_insert_header() {
        let mut req = create_req();

        req.try_insert_header::<Foo>("some key").unwrap();

        assert_eq!(req.get_header::<Foo>().unwrap().value(), "some key");
    }

    #[test]
    fn it_inserts_raw_header() {
        let mut req = create_req();

        req.insert_raw_header(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("some key"),
        );

        assert_eq!(req.headers().get("x-api-key").unwrap(), "some key");
    }

    #[test]
    fn it_tries_insert_raw_header() {
        let mut ctx = create_req();

        ctx.try_insert_raw_header("x-foo", "some key").unwrap();

        assert_eq!(ctx.get_header::<Foo>().unwrap().value(), "some key");
    }

    #[test]
    fn it_appends_header() {
        let mut req = create_req();

        let api_key_header: Header<Foo> = Header::from_static("1");
        let _ = req.append_header(api_key_header);

        let api_key_header: Header<Foo> = Header::from_static("2");
        let _ = req.append_header(api_key_header);

        assert_eq!(req.headers().get_all("x-foo").into_iter().collect::<Vec<_>>(), ["1", "2"]);
    }

    #[test]
    fn it_tries_append_header() {
        let mut req = create_req();

        req.try_append_header::<Foo>("1").unwrap();
        req.try_append_header::<Foo>("2").unwrap();

        assert_eq!(req.get_all_headers::<Foo>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]);
    }

    #[test]
    fn it_appends_raw_header() {
        let mut req = create_req();

        req.append_raw_header(
            HeaderName::from_static("x-foo"),
            HeaderValue::from_static("1"),
        );

        req.append_raw_header(
            HeaderName::from_static("x-foo"),
            HeaderValue::from_static("2"),
        );

        assert_eq!(req.get_all_headers::<Foo>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]);
    }

    #[test]
    fn it_tries_appends_raw_header() {
        let mut req = create_req();

        req.try_append_raw_header("x-foo", "1").unwrap();
        req.try_append_raw_header("x-foo", "2").unwrap();

        assert_eq!(req.get_all_headers::<Foo>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]);
    }

    #[test]
    fn it_removes_header() {
        let mut req = create_req();

        let header: Header<Foo> = Header::from_static("some key");
        let _ = req.insert_header(header);

        req.remove_header::<Foo>();

        assert!(req.headers().get("x-foo").is_none());
    }

    #[test]
    fn it_tries_remove_header() {
        let mut req = create_req();

        let api_key_header: Header<Foo> = Header::from_static("some key");
        let _ = req.insert_header(api_key_header);

        let result = req.try_remove_header("x-foo").unwrap();

        assert!(result);
        assert!(req.headers().get("x-api-key").is_none());
    }
}
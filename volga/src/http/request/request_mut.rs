//! Mutable version of [`HttpRequest`] for middleware

use crate::{
    HttpBody, HttpRequest,
    error::Error,
    headers::{FromHeaders, Header, HeaderMap, HeaderName, HeaderValue},
    http::{
        Extensions, FromRequestRef, Method, Parts, Uri, Version, request_scope::HttpRequestScope,
    },
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
/// During this phase the **headers may be modified** and the body
/// **replaced** ([`map_body`](Self::map_body)), but **not taken**: a
/// middleware that reads the body ([`try_map_body_async`](Self::try_map_body_async))
/// puts another in its place, and the request goes on with it.
///
/// To transition into the handler phase, the framework freezes the request,
/// producing an immutable [`HttpRequest`].
///
/// ## Design notes
///
/// - `HttpRequestMut` owns the underlying request
/// - It intentionally does **not** implement `Deref`
/// - No method takes the body out of the request: it is replaced, never removed
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

    /// Returns a mutable reference to the associated HTTP header map.
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

    /// Returns the scope of the request, for a middleware that writes to it
    #[inline]
    #[cfg(feature = "jwt-auth")]
    pub(crate) fn scope_mut(&mut self) -> Option<&mut HttpRequestScope> {
        self.inner.scope_mut()
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
    pub fn insert_header<T: FromHeaders>(&mut self, header: Header<T>) -> &mut Self {
        self.inner
            .headers_mut()
            .insert(header.name(), header.value().clone());
        self
    }

    /// Attempts to insert the header into the request, replacing any existing
    /// values with the same header name.
    ///
    /// Returns an error if the header cannot be constructed.
    #[inline]
    pub fn try_insert_header<T>(
        &mut self,
        header: impl TryInto<Header<T>, Error = Error>,
    ) -> Result<&mut Self, Error>
    where
        T: FromHeaders,
    {
        let header = header.try_into()?;
        Ok(self.insert_header(header))
    }

    /// Inserts the raw header into the request, replacing any existing values
    /// with the same header name.
    #[inline]
    pub fn insert_raw_header(&mut self, name: HeaderName, value: HeaderValue) -> &mut Self {
        self.inner.headers_mut().insert(name, value);
        self
    }

    /// Attempts to insert the raw header into the request, replacing any existing values
    /// with the same header name.
    #[inline]
    pub fn try_insert_raw_header(&mut self, name: &str, value: &str) -> Result<&mut Self, Error> {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(Error::from)?;
        let value = HeaderValue::from_str(value).map_err(Error::from)?;

        Ok(self.insert_raw_header(name, value))
    }

    /// Appends a new value for the given header name.
    ///
    /// Existing values with the same name are preserved.
    /// Multiple values for the same header may be present.
    #[inline]
    pub fn append_header<T>(&mut self, header: Header<T>) -> &mut Self
    where
        T: FromHeaders,
    {
        self.inner
            .headers_mut()
            .append(header.name(), header.value().clone());
        self
    }

    /// Attempts to append a new value for the given header name.
    ///
    /// Returns an error if the header cannot be constructed or appended.
    #[inline]
    pub fn try_append_header<T>(
        &mut self,
        header: impl TryInto<Header<T>, Error = Error>,
    ) -> Result<&mut Self, Error>
    where
        T: FromHeaders,
    {
        let header = header.try_into()?;
        Ok(self.append_header(header))
    }

    /// Appends a new raw value for the given raw header name.
    #[inline]
    pub fn append_raw_header(&mut self, name: HeaderName, value: HeaderValue) -> &mut Self {
        self.inner.headers_mut().append(name, value);
        self
    }

    /// Attempts to append a new raw value for the given header name.
    #[inline]
    pub fn try_append_raw_header(&mut self, name: &str, value: &str) -> Result<&mut Self, Error> {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(Error::from)?;
        let value = HeaderValue::from_str(value).map_err(Error::from)?;

        Ok(self.append_raw_header(name, value))
    }

    /// Removes all values for the given header name.
    ///
    /// Returns `true` if at least one header value was removed.
    #[inline]
    pub fn remove_header<T>(&mut self) -> bool
    where
        T: FromHeaders,
    {
        self.inner.headers_mut().remove(&T::NAME).is_some()
    }

    /// Attempts to remove all values for the given header name.
    ///
    /// Returns `true` if at least one value was removed.
    #[inline]
    pub fn try_remove_header(&mut self, name: &str) -> Result<bool, Error> {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(Error::from)?;

        Ok(self.inner.headers_mut().remove(name).is_some())
    }

    /// Returns the body limit of this request, in bytes, or `None` when it has none.
    ///
    /// See [`HttpRequest::body_limit`].
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

    /// Replaces the body of the request with what `f` makes of it.
    ///
    /// The new body is read under the body limit of the request, as the body it replaces was
    /// (see [`body_limit`](Self::body_limit)), so a body that grows here, such as a
    /// decompressed one, is still answered with `413` once it goes over the limit.
    ///
    /// The headers stay as they are. One that describes the old body, such as
    /// `Content-Length` or `Content-Encoding`, is removed with
    /// [`remove_header`](Self::remove_header) when it no longer holds.
    ///
    /// `f` does not read the body, it wraps it. To read it before putting one in its place,
    /// use [`try_map_body_async`](Self::try_map_body_async).
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpBody, HttpRequestMut};
    ///
    /// let mut app = App::new();
    ///
    /// app.tap_req(|req: HttpRequestMut| req.map_body(decode));
    ///
    /// # fn decode(body: HttpBody) -> HttpBody { body }
    /// ```
    #[inline]
    #[must_use]
    pub fn map_body<F>(mut self, f: F) -> Self
    where
        F: FnOnce(HttpBody) -> HttpBody,
    {
        let body = f(self.inner.take_body());
        self.inner.set_body(body);
        self
    }

    /// Replaces the body of the request with what `f` makes of it, or fails with the error
    /// `f` fails with.
    ///
    /// The error is [`Error`], as that of [`tap_req`](crate::App::tap_req), so that `?` and a
    /// bare `Ok(..)` need no type annotation; an error of another type gets there through `?`
    /// when it implements [`IntoError`](crate::error::IntoError). See
    /// [`map_body`](Self::map_body) for the body limit and the headers.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpBody, HttpRequestMut, error::Error};
    ///
    /// let mut app = App::new();
    ///
    /// app.tap_req(|req: HttpRequestMut| req.try_map_body(exotic_decompression));
    ///
    /// # fn exotic_decompression(body: HttpBody) -> Result<HttpBody, Error> {
    /// #     Ok(body)
    /// # }
    /// ```
    #[inline]
    pub fn try_map_body<F>(mut self, f: F) -> Result<Self, Error>
    where
        F: FnOnce(HttpBody) -> Result<HttpBody, Error>,
    {
        let body = f(self.inner.take_body())?;
        self.inner.set_body(body);
        Ok(self)
    }

    /// Replaces the body of the request with what the future of `f` makes of it, or fails with
    /// the error it fails with.
    ///
    /// It is the form that reads the body on the way: to check a signature over it, say, and
    /// put the bytes back for the handler. The body is read under the body limit of the
    /// request, so reading one over it fails with `413`, and the new body is read under the
    /// same limit. See [`try_map_body`](Self::try_map_body) for the error type and
    /// [`map_body`](Self::map_body) for the headers.
    ///
    /// There is no infallible form: what does not need the body is awaited in the middleware
    /// before this call, and reading the body can fail.
    ///
    /// # Example
    /// ```no_run
    /// use http_body_util::BodyExt;
    /// use volga::{App, HttpBody, HttpRequestMut, error::Error};
    ///
    /// let mut app = App::new();
    ///
    /// app.tap_req(|req: HttpRequestMut| async move {
    ///     req.try_map_body_async(|body| async move {
    ///         let bytes = body.collect().await?.to_bytes();
    ///         verify_signature(&bytes)?;
    ///         Ok(HttpBody::full(bytes))
    ///     })
    ///     .await
    /// });
    ///
    /// # fn verify_signature(_: &[u8]) -> Result<(), Error> {
    /// #     Ok(())
    /// # }
    /// ```
    // Not an `async fn`, which keeps `self` as a parameter and moves it to a second slot of
    // the future: 928 bytes against 496, for a request of 432
    #[inline]
    #[allow(clippy::manual_async_fn)]
    pub fn try_map_body_async<F, Fut>(mut self, f: F) -> impl Future<Output = Result<Self, Error>>
    where
        F: FnOnce(HttpBody) -> Fut,
        Fut: Future<Output = Result<HttpBody, Error>>,
    {
        async move {
            let body = f(self.inner.take_body()).await?;
            self.inner.set_body(body);
            Ok(self)
        }
    }

    /// Consumes the request and returns request head and body
    #[inline]
    #[deprecated(
        since = "0.15.0",
        note = "replace the body with `map_body`, `try_map_body` or `try_map_body_async`; the \
                head holds the request scope, which is not exposed"
    )]
    pub fn into_parts(self) -> (Parts, HttpBody) {
        self.inner.into_parts()
    }

    /// Creates a new `HttpRequest` with the given head and body
    #[inline]
    #[deprecated(
        since = "0.15.0",
        note = "replace the body with `map_body`, `try_map_body` or `try_map_body_async`; the \
                head holds the request scope, which is not exposed"
    )]
    pub fn from_parts(mut parts: Parts, body: HttpBody) -> Self {
        // `into_parts` left the scope in the extensions, where it lived before it was a field
        let scope = parts.extensions.remove::<HttpRequestScope>();
        let inner = HttpRequest::from_parts(parts, body);
        let inner = match scope {
            Some(scope) => inner.with_scope(scope),
            None => inner,
        };
        Self { inner }
    }
}

/// Conversion trait for values returned from `tap_req` middleware.
///
/// This trait exists solely to make `tap_req` middleware ergonomic:
/// a middleware may either return the request directly or fail with an error.
///
/// # Supported return types
///
/// * [`HttpRequestMut`] - returned as-is
/// * `Result<HttpRequestMut, Error>` - propagated without modification
///
/// The error type is `Error` alone, unlike a handler's, which takes any
/// [`IntoError`](crate::error::IntoError): with one `Result` to convert from, `?` and a bare
/// `Ok(..)` need no type annotation. An error of another type gets there through `?` when it
/// implements [`IntoError`](crate::error::IntoError), which gives `From<T> for Error`, or as
/// `Err(err.into())`.
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
/// app.tap_req(|req: HttpRequestMut| {
///     let mut req = req.try_map_body(exotic_decompression)?;
///     req.try_remove_header("content-encoding")?;
///
///     Ok(req)
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
pub trait IntoTapResult: sealed::Sealed {
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
#[allow(unused)]
mod tests {
    use super::*;
    use crate::HttpBody;
    use crate::headers::headers;
    use crate::http::Request;
    use crate::http::endpoints::route::{PathArg, PathArgs};
    use crate::http::request::request_body_limit::RequestBodyLimit;
    use bytes::Bytes;
    use http_body_util::BodyExt;

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

    /// A request whose body is `body` and whose body limit is `limit`, applied to the body as
    /// a request is given it when it is routed
    fn create_req_with(limit: RequestBodyLimit, body: &'static str) -> HttpRequestMut {
        use crate::http::request_scope::HttpRequestScope;

        let scope = HttpRequestScope {
            body_limit: limit,
            ..HttpRequestScope::default()
        };
        let (parts, body) = Request::post("/")
            .body(HttpBody::full(body))
            .unwrap()
            .into_parts();

        HttpRequestMut::new(
            HttpRequest::from_parts(parts, body)
                .with_scope(scope)
                .into_limited(limit),
        )
    }

    async fn read_body(req: HttpRequestMut) -> Result<Bytes, Error> {
        Ok(req.freeze().into_body().collect().await?.to_bytes())
    }

    #[test]
    fn it_debugs() {
        let req = create_req();
        assert_eq!(format!("{req:?}"), "HttpRequestMut(..)");
    }

    #[test]
    fn it_returns_url_path() {
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

        let scope = HttpRequestScope {
            params: args,
            ..HttpRequestScope::default()
        };
        let req = Request::get("/").body(HttpBody::empty()).unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequestMut::new(HttpRequest::from_parts(parts, body).with_scope(scope));

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

        assert_eq!(
            req.headers()
                .get_all("x-foo")
                .into_iter()
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
    }

    #[test]
    fn it_tries_append_header() {
        let mut req = create_req();

        req.try_append_header::<Foo>("1").unwrap();
        req.try_append_header::<Foo>("2").unwrap();

        assert_eq!(
            req.get_all_headers::<Foo>()
                .map(|h| h.into_inner())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
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

        assert_eq!(
            req.get_all_headers::<Foo>()
                .map(|h| h.into_inner())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
    }

    #[test]
    fn it_tries_appends_raw_header() {
        let mut req = create_req();

        req.try_append_raw_header("x-foo", "1").unwrap();
        req.try_append_raw_header("x-foo", "2").unwrap();

        assert_eq!(
            req.get_all_headers::<Foo>()
                .map(|h| h.into_inner())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
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

    #[tokio::test]
    async fn it_hands_the_body_to_map_body() {
        let req = create_req_with(RequestBodyLimit::Disabled, "hello").map_body(|body| body);

        assert_eq!(read_body(req).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn it_replaces_the_body() {
        let req = create_req_with(RequestBodyLimit::Disabled, "hello")
            .map_body(|_| HttpBody::full("replaced"));

        assert_eq!(read_body(req).await.unwrap(), "replaced");
    }

    #[tokio::test]
    async fn it_tries_to_replace_the_body() {
        let req = create_req_with(RequestBodyLimit::Disabled, "hello")
            .try_map_body(|_| Ok(HttpBody::full("replaced")))
            .unwrap();

        assert_eq!(read_body(req).await.unwrap(), "replaced");
    }

    #[test]
    fn it_fails_with_the_error_of_try_map_body() {
        let err = create_req_with(RequestBodyLimit::Disabled, "hello")
            .try_map_body(|_| Err(Error::client_error("cannot decode")))
            .unwrap_err();

        assert_eq!(err.status(), 400);
        assert_eq!(err.to_string(), "cannot decode");
    }

    #[tokio::test]
    async fn it_reads_a_replaced_body_under_the_body_limit() {
        let req = create_req_with(RequestBodyLimit::Enabled(8), "hello")
            .map_body(|_| HttpBody::full("longer than eight bytes"));

        assert!(read_body(req).await.is_err());

        let req = create_req_with(RequestBodyLimit::Enabled(8), "hello")
            .try_map_body(|_| Ok(HttpBody::full("longer than eight bytes")))
            .unwrap();

        assert!(read_body(req).await.is_err());
    }

    #[tokio::test]
    async fn it_reads_a_replaced_body_whole_without_a_body_limit() {
        let req = create_req_with(RequestBodyLimit::Disabled, "hello")
            .map_body(|_| HttpBody::full("longer than eight bytes"));

        assert_eq!(read_body(req).await.unwrap(), "longer than eight bytes");
    }

    #[test]
    #[allow(deprecated)]
    fn it_keeps_the_scope_through_into_parts_and_from_parts() {
        let args: PathArgs = smallvec::smallvec![PathArg {
            name: "id".into(),
            value: "7".into()
        }]
        .into();
        let scope = HttpRequestScope {
            params: args,
            body_limit: RequestBodyLimit::Enabled(8),
            ..HttpRequestScope::default()
        };
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        let req = HttpRequestMut::new(HttpRequest::from_parts(parts, body).with_scope(scope));

        let (parts, body) = req.into_parts();
        let req = HttpRequestMut::from_parts(parts, body);

        assert_eq!(req.path_args().collect::<Vec<_>>(), [("id", "7")]);
        assert_eq!(req.body_limit(), Some(8));
        assert!(req.extensions().get::<HttpRequestScope>().is_none());
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn it_keeps_into_parts_and_from_parts_working() {
        let (parts, _) = create_req_with(RequestBodyLimit::Disabled, "hello").into_parts();
        let req = HttpRequestMut::from_parts(parts, HttpBody::full("replaced"));

        assert_eq!(req.uri(), "/");
        assert_eq!(read_body(req).await.unwrap(), "replaced");
    }

    #[tokio::test]
    async fn it_replaces_the_body_with_what_it_reads_of_it() {
        let req = create_req_with(RequestBodyLimit::Disabled, "hello")
            .try_map_body_async(|body| async move {
                let bytes = body.collect().await?.to_bytes();
                Ok(HttpBody::full(bytes.to_ascii_uppercase()))
            })
            .await
            .unwrap();

        assert_eq!(read_body(req).await.unwrap(), "HELLO");
    }

    #[tokio::test]
    async fn it_fails_with_the_error_of_try_map_body_async() {
        let err = create_req_with(RequestBodyLimit::Disabled, "hello")
            .try_map_body_async(|_| async { Err(Error::client_error("bad signature")) })
            .await
            .unwrap_err();

        assert_eq!(err.status(), 400);
        assert_eq!(err.to_string(), "bad signature");
    }

    #[tokio::test]
    async fn it_reads_the_body_under_the_body_limit_in_try_map_body_async() {
        let err = create_req_with(RequestBodyLimit::Enabled(4), "hello")
            .try_map_body_async(|body| async move {
                let bytes = body.collect().await?.to_bytes();
                Ok(HttpBody::full(bytes))
            })
            .await
            .unwrap_err();

        assert_eq!(err.status(), 413);
    }

    #[tokio::test]
    async fn it_reads_a_body_replaced_asynchronously_under_the_body_limit() {
        let req = create_req_with(RequestBodyLimit::Enabled(8), "hello")
            .try_map_body_async(|_| async { Ok(HttpBody::full("longer than eight bytes")) })
            .await
            .unwrap();

        assert!(read_body(req).await.is_err());
    }
}

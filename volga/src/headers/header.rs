//! Extractors for HTTP headers

use futures_util::future::{Ready, ready};
use hyper::{http::request::Parts, header::AsHeaderName};
use crate::{HttpRequest, error::Error};

use super::{FromHeaders, HeaderMap, HeaderValue, HeaderError, HeaderName};

use crate::http::endpoints::args::{
    FromPayload, 
    FromRequestParts,
    FromRequestRef,
    Payload,
    Source
};

use std::{
    fmt::{Display, Formatter},
    marker::PhantomData,
};

pub use try_into_header::TryIntoHeaderPair;

mod try_into_header;

/// Represents a snapshot of HTTP request headers.
///
/// This type is intended for read-only access and does not allow
/// mutating the underlying request headers. 
/// For mutating headers, use [`HttpRequest`] extractor instead.
/// 
/// # Example
/// ```no_run
/// use volga::{HttpResult, ok};
/// use volga::headers::{HttpHeaders, Header, ContentLength};
///
/// async fn handle(headers: HttpHeaders) -> HttpResult {
///     let content_length: Header<ContentLength> = headers.try_get()?;
///     ok!("Content-Length: {content_length}")
/// }
/// ```
#[derive(Debug, Clone)]
pub struct HttpHeaders {
    inner: HeaderMap<HeaderValue>
}

impl HttpHeaders {
    /// Returns a raw HTTP header value by name
    #[inline]
    pub fn get_raw(&self, name: impl AsHeaderName) -> Option<&HeaderValue> {
        self.inner.get(name)
    }

    /// Returns a typed HTTP header value
    #[inline]
    pub fn get<T: FromHeaders>(&self) -> Option<Header<T>> {
        T::from_headers(&self.inner).map(Header::from_ref)
    }

    /// Returns a typed HTTP header value or an error
    #[inline]
    pub fn try_get<T: FromHeaders>(&self) -> Result<Header<T>, Error> {
        self.get::<T>()
            .ok_or_else(HeaderError::header_missing::<T>)
    }

    /// Returns a view of all values associated with this HTTP header.
    #[inline]
    pub fn get_all<T: FromHeaders>(&self) -> impl Iterator<Item = Header<T>> {
        self.inner
            .get_all(T::NAME)
            .iter()
            .map(Header::from_ref)
    }

    /// Returns a view of all rawvalues associated with this HTTP header.
    #[inline]
    pub fn get_all_raw(&self, name: impl AsHeaderName) -> impl Iterator<Item = &HeaderValue> {
        self.inner.get_all(name).iter()
    }
}

impl From<HeaderMap<HeaderValue>> for HttpHeaders {
    #[inline]
    fn from(inner: HeaderMap<HeaderValue>) -> Self {
        Self { inner }
    }
}

impl From<&Parts> for HttpHeaders {
    #[inline]
    fn from(parts: &Parts) -> Self {
        parts.headers
            .clone()
            .into()
    }
}

/// Extracts `HeadersMap` from request parts into [`HttpHeaders`]
impl FromRequestParts for HttpHeaders {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.into())
    }
}

/// Extracts `HeaderValue` from request into `Header<T>``
/// where T implements [`FromHeaders`] `struct`
impl FromRequestRef for HttpHeaders {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Ok(req.headers().clone().into())
    }
}

/// Extracts `HeaderMap` from request into `Headers`
impl FromPayload for HttpHeaders {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }
}

/// Typed header that wraps a [`HeaderValue`]
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, ok};
/// use volga::headers::{Header, ContentType};
///
/// async fn handle(content_type: Header<ContentType>) -> HttpResult {
///     ok!("Content-Type: {content_type}")
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Header<T: FromHeaders> {
    value: HeaderValue,
    _marker: PhantomData<T>
}

impl<T: FromHeaders> AsRef<HeaderValue> for Header<T> {
    #[inline]
    fn as_ref(&self) -> &HeaderValue {
        self.value()
    }
}

impl<T: FromHeaders> From<HeaderValue> for Header<T> {
    #[inline]
    fn from(value: HeaderValue) -> Self {
        Self { 
            value, 
            _marker: PhantomData
        }
    }
}

impl<T: FromHeaders> TryFrom<&str> for Header<T> {
    type Error = Error;
    
    #[inline]
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let header_value = HeaderValue::from_str(value)
            .map_err(HeaderError::from_invalid_header_value)?;
        Ok(Self { value: header_value, _marker: PhantomData })
    }
}

impl<T: FromHeaders> Display for Header<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.value.to_str() {
            Ok(v) => write!(f, "{}: {}", self.name(), v),
            Err(_) => write!(f, "{}: <binary>", self.name()),
        }
    }
}

impl<T: FromHeaders> Header<T> {
    /// Creates a new instance of [`Header<T>`] from [`HeaderValue`]
    pub fn new(header_value: HeaderValue) -> Self {
        header_value.into()
    }

    /// Creates a new instance of [`Header<T>`] from a reference to [`HeaderValue`]
    pub fn from_ref(header_value: &HeaderValue) -> Self {
        Self::new(header_value.clone())
    }

    /// Creates a new instance of [`Header<T>`] from a `static str`
    ///
    /// # Examples
    /// ```no_run
    /// use volga::headers::{ContentType, Header};
    ///
    /// let content_type_header = Header::<ContentType>::from_static("text/plain");
    /// assert_eq!(content_type_header.as_ref(), "text/plain");
    /// ```
    #[inline]
    pub const fn from_static(value: &'static str) -> Self {
        Self {
            value: HeaderValue::from_static(value),
            _marker: PhantomData
        }
    }
    
    /// Creates a new instance of [`Header<T>`] from a byte slice
    ///
    /// # Examples
    /// ```no_run
    /// use volga::headers::{ContentType, Header};
    ///
    /// let content_type_header = Header::<ContentType>::from_bytes(b"text/plain").unwrap();
    /// assert_eq!(content_type_header.as_ref(), "text/plain");
    /// ```
    /// An invalid value
    /// ```no_run
    /// use volga::headers::{ContentType, Header};
    ///
    /// let content_type_header = Header::<ContentType>::from_bytes(b"\n");
    /// assert!(content_type_header.is_err())
    /// ```
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let header_value = HeaderValue::from_bytes(bytes)
            .map_err(HeaderError::from_invalid_header_value)?;
        Ok(Self { value: header_value, _marker: PhantomData })
    }

    /// Unwraps the inner [`HeaderValue`]
    #[allow(unused)]
    pub(crate) fn into_inner(self) -> HeaderValue {
        self.value
    }

    /// Returns the canonical header name.
    #[inline]
    pub fn name(&self) -> HeaderName {
        T::NAME
    }

    /// Returns the raw header value.
    #[inline]
    pub fn value(&self) -> &HeaderValue {
        &self.value
    }

    /// Returns the header value as a string slice.
    ///
    /// Fails if the value is not valid ASCII.
    #[inline]
    pub fn as_str(&self) -> Result<&str, Error> {
        self.value
            .to_str()
            .map_err(HeaderError::from_to_str_error)
    }

    /// Returns the header value as raw bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.value.as_bytes()
    }

    /// Parses specific [`Header<T>`] from ['HeaderMap']
    #[inline]
    pub(super) fn from_headers_map(headers: &HeaderMap) -> Result<Self, Error> {
        T::from_headers(headers)
            .ok_or_else(HeaderError::header_missing::<T>)
            .map(Self::from_ref)
    }
}

/// Extracts `HeaderValue` from request parts into `Header<T>``
/// where T implements [`FromHeaders`] `struct`
impl<T: FromHeaders + Send> FromRequestParts for Header<T> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Self::from_headers_map(&parts.headers)
    }
}

/// Extracts `HeaderValue` from request into `Header<T>``
/// where T implements [`FromHeaders`] `struct`
impl<T: FromHeaders + Send> FromRequestRef for Header<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Self::from_headers_map(req.headers())
    }
}

/// Extracts `HeaderValue` from request parts into `Header<T>``
/// where T implements [`FromHeaders`] `struct`
impl<T: FromHeaders + Send> FromPayload for Header<T> {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }
}

#[cfg(test)]
mod tests {
    use hyper::{HeaderMap, Request};
    use hyper::http::HeaderValue;
    use crate::{HttpBody, HttpRequest};
    use crate::headers::{ContentType, Header, HttpHeaders};
    use crate::http::{FromRequestParts, FromRequestRef};
    use crate::http::endpoints::args::{FromPayload, Payload};

    #[test]
    fn it_creates_http_header_from_header_map() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/plain"));

        let headers = HttpHeaders::from(headers);

        assert_eq!(headers.get_raw("Content-Type").unwrap(), "text/plain");
    }

    #[test]
    fn it_gets_header_from_http_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/plain"));

        let headers = HttpHeaders::from(headers);

        assert_eq!(headers.get::<ContentType>().unwrap().value(), "text/plain");
    }

    #[test]
    fn it_tries_get_header_from_http_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/plain"));

        let headers = HttpHeaders::from(headers);

        let header = headers.try_get::<ContentType>();

        assert_eq!(header.unwrap().value(), "text/plain");
    }

    #[test]
    fn it_trie_get_missing_header_from_http_headers() {
        let headers = HeaderMap::new();
        let headers = HttpHeaders::from(headers);

        let header = headers.try_get::<ContentType>();

        assert!(header.is_err());
    }

    #[test]
    fn it_gets_all_headers_from_http_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/plain"));

        let headers = HttpHeaders::from(headers);

        assert_eq!(headers.get_all::<ContentType>().map(|h| h.value().clone()).collect::<Vec<_>>(), ["text/plain"]);
    }

    #[test]
    fn it_gets_all_raw_headers_from_http_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/plain"));

        let headers = HttpHeaders::from(headers);

        assert_eq!(headers.get_all_raw("Content-Type").collect::<Vec<_>>(), ["text/plain"]);
    }

    #[tokio::test]
    async fn it_reads_headers_from_payload() {
        let req = Request::get("/")
            .header("Content-Type", HeaderValue::from_static("text/plain"))
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        let headers = HttpHeaders::from_payload(Payload::Parts(&parts)).await.unwrap();

        assert_eq!(headers.get_raw("content-type").unwrap(), "text/plain");
    }

    #[test]
    fn it_reads_headers_from_parts() {
        let req = Request::get("/")
            .header("Content-Type", HeaderValue::from_static("text/plain"))
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        let headers = HttpHeaders::from_parts(&parts).unwrap();

        assert_eq!(headers.get_raw("content-type").unwrap(), "text/plain");
    }

    #[test]
    fn it_reads_headers_from_request_ref() {
        let req = Request::get("/")
            .header("Content-Type", HeaderValue::from_static("text/plain"))
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let headers = HttpHeaders::from_request(&req).unwrap();

        assert_eq!(headers.get_raw("content-type").unwrap(), "text/plain");
    }

    #[test]
    fn it_reads_header_from_parts() {
        let req = Request::get("/")
            .header("Content-Type", HeaderValue::from_static("text/plain"))
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        let header: Header<ContentType> = Header::from_parts(&parts).unwrap();

        assert_eq!(header.as_ref(), "text/plain");
    }

    #[test]
    fn it_reads_header_from_request_ref() {
        let req = Request::get("/")
            .header("Content-Type", HeaderValue::from_static("text/plain"))
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let header: Header<ContentType> = Header::from_request(&req).unwrap();

        assert_eq!(header.as_ref(), "text/plain");
    }

    #[tokio::test]
    async fn it_reads_header_from_payload() {
        let req = Request::get("/")
            .header("Content-Type", HeaderValue::from_static("text/plain"))
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        let header: Header<ContentType> = Header::from_payload(Payload::Parts(&parts)).await.unwrap();

        assert_eq!(header.as_ref(), "text/plain");
    }

    #[test]
    fn it_gets_header() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/plain"));

        let header: Header<ContentType> = Header::from_headers_map(&headers).unwrap();

        assert_eq!(header.as_ref(), "text/plain");
    }

    #[test]
    fn it_gets_missing_header() {
        let headers = HeaderMap::new();

        let header = Header::<ContentType>::from_headers_map(&headers);

        assert!(header.is_err());
        assert_eq!(header.err().unwrap().to_string(), "Header: `content-type` not found");
    }

    #[test]
    fn i_creates_header_from_bytes() {
        let header_value_bytes = b"text/plain";

        let header = Header::<ContentType>::from_bytes(header_value_bytes).unwrap();

        assert_eq!(header.value(), "text/plain");
    }

    #[test]
    fn i_creates_header_from_str() {
        let header_value = "text/plain";

        let header = Header::<ContentType>::try_from(header_value).unwrap();

        assert_eq!(header.value(), "text/plain");
    }

    #[test]
    fn i_creates_header_from_static() {
        let header = Header::<ContentType>::from_static("text/plain");

        assert_eq!(header.value(), "text/plain");
    }

    #[test]
    fn it_creates_parts() {
        let header = Header::<ContentType>::from_static("text/plain");

        let (name, value) = (header.name(), header.value());

        assert_eq!(name, "content-type");
        assert_eq!(value, "text/plain");
    }

    #[test]
    fn it_returns_invalid_header_error() {
        let header = Header::<ContentType>::try_from("\\FF\x0000");

        assert!(header.is_err());
        assert_eq!(header.err().unwrap().to_string(), "Header: failed to parse header value");
    }
}
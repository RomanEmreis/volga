//! HTTP response builder macro definition

use std::fmt::{Debug, Formatter};
use crate::{
    error::Error,
    headers::{Header, HeaderName, HeaderValue, HeaderMap, FromHeaders},
    http::{HttpBody, HttpResponse, Response, StatusCode}
};

/// Default server name
pub const SERVER_NAME: &str = "Volga";
/// Default resource builder error
pub const RESPONSE_ERROR: &str = "HTTP Response: Unable to create a response";

/// Builder for [`HttpResponse`].
///
/// This type provides a controlled way to construct HTTP responses
/// while preserving framework-level invariants.
pub struct HttpResponseBuilder {
    inner: Result<InnerBuilder, Error>
}

/// The inner builder representation
struct InnerBuilder {
    status: StatusCode,
    headers: HeaderMap,
}

impl Debug for HttpResponseBuilder {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpResponseBuilder(...)").finish()
    }
}

impl HttpResponseBuilder {
    /// Creates a new [`HttpResponseBuilder`]
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            inner: Ok(InnerBuilder {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
            })
        }
    }

    /// Sets the HTTP status code.
    #[inline]
    pub fn status<T>(self, status: T) -> Self
    where
        StatusCode: TryFrom<T>,
        Error: From<<StatusCode as TryFrom<T>>::Error>,
    {
        self.and_then(|mut inner| {
            inner.status = status
                .try_into()
                .map_err(Error::from)?;
        
            Ok(inner)
        })
    }

    /// Appends an HTTP header value.
    ///
    /// If a header with the same name already exists, the value is appended
    ///
    /// > **Note:** This may result in multiple values for the same header.
    #[inline]
    pub fn header<T>(self, header: impl TryInto<Header<T>, Error = impl Into<Error>>) -> Self
    where 
        T: FromHeaders
    {
        self.and_then(move |mut inner| {
            let header = header
                .try_into()
                .map_err(Into::into)?;

            inner.headers
                .try_append(T::NAME, header.into_inner())
                .map_err(Error::from)?;
            Ok(inner)
        })
    }

    /// Appends an HTTP header value.
    ///
    /// If a header with the same name already exists, the value is appended
    ///
    /// > **Note:** This may result in multiple values for the same header.
    #[inline]
    pub fn header_raw<K, V>(self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        HeaderValue: TryFrom<V>,
        Error: From<<HeaderName as TryFrom<K>>::Error>,
        Error: From<<HeaderValue as TryFrom<V>>::Error>,
    {
        self.and_then(|mut inner| {
            let name = HeaderName::try_from(key).map_err(Error::from)?;
            let value = HeaderValue::try_from(value).map_err(Error::from)?;
            
            inner.headers
                .try_append(name, value)
                .map_err(Error::from)?;
            
            Ok(inner)
        })
    }

    /// Appends an HTTP header value from a static source.
    ///
    /// If a header with the same name already exists, the value is appended
    ///
    /// > **Note:** This may result in multiple values for the same header.
    #[inline]
    pub fn header_static(self, key: &'static str, value: &'static str) -> Self {
        self.and_then(|mut inner| {
            let name = HeaderName::from_static(key);
            let value = HeaderValue::from_static(value);
            inner.headers.append(name, value);
            Ok(inner)
        })
    }

    /// Finalizes the response with the given body.
    ///
    /// # Errors
    /// Returns an error if the response cannot be constructed.
    #[inline]
    pub fn body(self, body: HttpBody) -> Result<HttpResponse, Error> {
        self.inner.and_then(|inner| {
            let mut response = Response::builder()
                .status(inner.status)
                .body(body)
                .map_err(|_| Error::server_error(RESPONSE_ERROR))?;

            *response.headers_mut() = inner.headers;
        
            Ok(HttpResponse::from_inner(response))
        })
    }

    #[inline]
    fn and_then<F>(self, func: F) -> Self
    where
        F: FnOnce(InnerBuilder) -> Result<InnerBuilder, Error>,
    {
        Self {
            inner: self.inner.and_then(func),
        }
    }
}

/// Creates a response builder
#[inline]
#[cfg(debug_assertions)]
pub fn make_builder() -> HttpResponseBuilder {
    HttpResponse::builder()
        .header_raw(crate::headers::SERVER, SERVER_NAME)
}

/// Creates a response builder
#[inline]
#[cfg(not(debug_assertions))]
pub fn make_builder() -> HttpResponseBuilder {
    HttpResponse::builder()
}

/// Creates a default HTTP response builder
#[macro_export]
macro_rules! builder {
    () => {
        $crate::http::response::builder::make_builder()
    };
    ($status:expr) => {
        $crate::builder!()
            .status($status)
    };
}

/// Creates an HTTP response with `status`, `body` and `headers`
#[macro_export]
macro_rules! response {
    ($status:expr, $body:expr) => {
        $crate::response!($status, $body; [])
    };
    ($status:expr, $body:expr; [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::builder!($status)
        $(
            .header_raw($key, $value)
        )*
            .body($body)
    };
    ($status:expr, $body:expr; [ $( $header:expr ),* $(,)? ]) => {
        $crate::builder!($status)
        $(
            .header($header)
        )*
            .body($body)
    };
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use crate::HttpBody;
    use crate::headers::{Header, ContentType};
    use super::RESPONSE_ERROR;

    #[tokio::test]
    async fn builder_sets_status_headers_and_body() {
        let response = builder!(200)
            .header::<ContentType>("text/plain")
            .body(HttpBody::from("hello"))
            .expect("response should build");

        let response = response.into_inner();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain");

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, "hello");
    }

    #[tokio::test]
    async fn it_creates_response_with_headers_and_body() {
        let header = Header::<ContentType>::from_static("text/plain");
        let response = response!(
            200, 
            HttpBody::from("hello");
            [header]
        );

        let response = response.expect("response should build").into_inner();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/plain");

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, "hello");
    }

    #[tokio::test]
    async fn builder_sets_status_headers_raw_and_body() {
        let response = builder!(201)
            .header_raw("x-test", "1")
            .body(HttpBody::from("hello"))
            .expect("response should build");

        let response = response.into_inner();
        assert_eq!(response.status(), 201);
        assert_eq!(response.headers().get("x-test").unwrap(), "1");

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, "hello");
    }

    #[tokio::test]
    async fn response_macro_builds_with_body() {
        let response = response!(200, HttpBody::from("ok")).expect("response should build");
        let response = response.into_inner();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, "ok");
    }

    #[test]
    fn builder_returns_error_for_invalid_header() {
        let result = builder!()
            .header_raw("invalid header", "value")
            .body(HttpBody::from("ignored"));

        let err = result.expect_err("expected invalid header error");
        assert!(err.to_string().contains(RESPONSE_ERROR) || err.to_string().contains("header"));
    }
}
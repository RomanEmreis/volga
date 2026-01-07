//! HTTP response utilities
 
use crate::response;
use crate::error::Error;
use crate::http::{
    body::{BoxBody, HttpBody},
    Extensions,
    StatusCode,
    Version
};

use std::collections::HashMap;
use tokio::fs::File;
use serde::Serialize;

use hyper::{
    header::{
        IntoHeaderName,
        HeaderMap,
        HeaderName, 
        HeaderValue,
        CONTENT_DISPOSITION, 
        CONTENT_TYPE, 
        TRANSFER_ENCODING
    }, 
    http, 
    body::{Body, SizeHint},
    http::response::Parts, 
    Response,
};

use mime::{
    APPLICATION_JSON,
    APPLICATION_OCTET_STREAM,
    TEXT_PLAIN
};

use crate::headers::{FromHeaders, Header, HeaderError};

pub use builder::HttpResponseBuilder;

pub mod builder;
pub mod macros;
pub mod ok;
pub mod form;
pub mod file;
pub mod stream;
pub mod status;
pub mod into_response;
pub mod redirect;
pub mod html;
pub mod sse;
#[cfg(feature = "middleware")]
pub mod filter_result;

/// A customized response context with custom response `headers` and `content_type`
/// > NOTE: This is not suitable for file response use the `file!` or `Results::file()` instead
/// # Example
/// ```no_run
/// use volga::{Results, ResponseContext};
/// use std::collections::HashMap;
/// use std::result;
///
/// let mut headers = HashMap::new();
/// headers.insert(String::from("x-api-key"), String::from("some api key"));
///
/// let result = Results::from(ResponseContext {
///     content: "Hello World!",
///     status: 200,
///     headers
/// });
/// ```
/// or alternative way by using `From` trait
/// ```no_run
/// use volga::{ResponseContext, HttpResult};
/// use std::collections::HashMap;
///
/// let mut headers = HashMap::new();
/// headers.insert(String::from("x-api-key"), String::from("some api key"));
///
/// let result = HttpResult::from(ResponseContext {
///     content: "Hello World!",
///     status: 200,
///     headers
/// });
/// ```
#[derive(Debug)]
pub struct ResponseContext<T: Serialize> {
    /// Response content
    pub content: T,

    /// HTTP response status code
    pub status: u16,

    /// HTTP response headers
    pub headers: HashMap<String, String>
}

/// Represents an HTTP response
/// 
/// See [`Response`]
pub struct HttpResponse {
    inner: Response<HttpBody>
}

impl std::fmt::Debug for HttpResponse {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpResponse(..)")
    }
}

/// Represents a result of HTTP request that could be 
/// either [`HttpResponse`] or [`Error`]
pub type HttpResult = Result<HttpResponse, Error>;

impl From<HttpResponse> for Response<HttpBody> {
    #[inline]
    fn from(resp: HttpResponse) -> Self {
        resp.into_inner()
    }
}

impl HttpResponse {
    /// Creates a new [`HttpResponseBuilder`] with default settings.
    ///
    /// By default:
    /// - status is set to `200 OK`
    /// - no headers are set
    #[inline]
    pub fn builder() -> HttpResponseBuilder {
        HttpResponseBuilder {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
        }
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
    pub fn status(&self) -> StatusCode {
        self.inner.status()
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

    /// Returns a reference to the associated HTTP body.
    #[inline]
    pub fn body(&self) -> &HttpBody {
        self.inner.body()
    }

    /// Returns the bounds on the remaining length of the stream.
    /// 
    /// When the exact remaining length of the stream is known, 
    /// the upper bound will be set and will equal the lower bound.
    #[inline]
    pub fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
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
            .map_err(HeaderError::from_invalid_header_name)?;

        Ok(self.inner.headers_mut().remove(name).is_some())
    }

    /// Returns a mutable reference to the associated HTTP body.
    #[inline]
    pub(crate) fn body_mut(&mut self) -> &mut HttpBody {
        self.inner.body_mut()
    }
    
    /// Unwraps the inner request
    #[inline]
    pub(crate) fn into_inner(self) -> Response<HttpBody> {
        self.inner
    }
    
    /// Consumes the response returning the head and body parts.
    #[inline]
    #[allow(unused)]
    pub(crate) fn into_parts(self) -> (Parts, HttpBody) {
        self.inner.into_parts()
    }

    /// Creates a new [`HttpResponse`] with the given head and body
    #[inline]
    #[allow(unused)]
    pub(crate) fn from_parts(parts: Parts, body: HttpBody) -> Self {
        Self { inner: Response::from_parts(parts, body) }
    }

    /// Creates a new [`HttpResponse`] from [`Response<HttpBody>`] 
    #[inline]
    pub(crate) fn from_inner(inner: Response<HttpBody>) -> Self {
        Self { inner }
    }
}

/// Results helpers
#[allow(missing_debug_implementations)]
pub struct Results;

impl Results {
    /// Inserts a key-value pairs as HTTP headers for the [`HttpResult`] if it is [`Ok`].
    ///
    /// Otherwise, leaves the [`Err`] as is.
    #[inline]
    pub fn with_headers<K, V, const N: usize>(res: HttpResult, headers: [(K, V); N]) -> HttpResult
    where
        K: IntoHeaderName,
        V: TryInto<HeaderValue>,
        <V as TryInto<HeaderValue>>::Error: Into<http::Error>,
    {
        match res {
            Err(err) => Err(err),
            Ok(mut res) => {
                let headers_mut = res.headers_mut();
                for (k, v) in headers.into_iter() {
                    match v.try_into() { 
                        Ok(v) => headers_mut.insert(k, v),
                        Err(err) => return Err(Error::from(err.into()))
                    };
                }
                Ok(res)
            },
        }
    }
    
    /// Inserts a key-value pair as an HTTP header for the [`HttpResult`] if it is [`Ok`].
    /// 
    /// Otherwise, leaves the [`Err`] as is.
    #[inline]
    pub fn with_header<K, V>(res: HttpResult, key: K, value: V) -> HttpResult
    where 
        K: IntoHeaderName,
        V: TryInto<HeaderValue>,
        <V as TryInto<HeaderValue>>::Error: Into<http::Error>,
    {
        match res {
            Err(err) => Err(err),
            Ok(mut res) => {
                let value = value
                    .try_into()
                    .map_err(|err| Error::from(err.into()))?;
                res.headers_mut().insert(key, value);
                Ok(res)
            },
        }
    }
    
    /// Produces a customized `OK 200` response
    #[inline]
    pub fn from<T: Serialize>(context: ResponseContext<T>) -> HttpResult {
        HttpResult::from(context)
    }

    /// Produces an `OK 200` response with the `JSON` body.
    #[inline]
    pub fn json<T>(content: T) -> HttpResult
    where 
        T: Serialize
    {
        Self::json_with_status(StatusCode::OK, content)
    }

    /// Produces a response with `StatusCode` the `JSON` body.
    #[inline]
    pub fn json_with_status<T>(status: StatusCode, content: T) -> HttpResult
    where 
        T: Serialize
    {
        response!(
            status,
            HttpBody::json(content),
            [
                (CONTENT_TYPE, APPLICATION_JSON.as_ref())
            ]
        )
    }

    /// Produces an `OK 200` response with the plain text body.
    #[inline]
    pub fn text(content: &str) -> HttpResult {
        response!(
            StatusCode::OK, 
            HttpBody::full(content.to_string()),
            [
                (CONTENT_TYPE, TEXT_PLAIN.as_ref())
            ]
        )
    }

    /// Produces an `OK 200` response with the stream body.
    #[inline]
    pub fn stream(content: BoxBody) -> HttpResult {
        response!(StatusCode::OK, HttpBody::new(content))
    }

    /// Produces an `OK 200` response with the file body.
    #[inline]
    pub fn file(file_name: &str, content: File) -> HttpResult {
        let boxed_body = HttpBody::file(content);
        let file_name = format!("attachment; filename=\"{file_name}\"");
        response!(
            StatusCode::OK, 
            boxed_body,
            [
                (CONTENT_TYPE, APPLICATION_OCTET_STREAM.as_ref()),
                (TRANSFER_ENCODING, "chunked"),
                (CONTENT_DISPOSITION, file_name)
            ]
        )
    }

    /// Produces an empty `OK 200` response.
    #[inline]
    pub fn ok() -> HttpResult {
        response!(
            StatusCode::OK, 
            HttpBody::empty(),
            [
                (CONTENT_TYPE, TEXT_PLAIN.as_ref())
            ]
        )
    }

    /// Produces a ` CLIENT CLOSED REQUEST 499 ` response.
    #[inline]
    pub fn client_closed_request() -> HttpResult {
        response!(
            StatusCode::from_u16(499).unwrap(),
            HttpBody::empty(),
            [(CONTENT_TYPE, TEXT_PLAIN.as_ref())])
    }
}

impl<T: Serialize> From<ResponseContext<T>> for HttpResult {
    #[inline]
    fn from(value: ResponseContext<T>) -> Self {
        let ResponseContext { content, headers, status } = value;
        let content = serde_json::to_vec(&content)?;
        let status = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);

        let mut resp = HttpResponse::builder()
            .status(status)?
            .body(HttpBody::full(content))?;
        
        let header_map = resp.headers_mut();
        headers.into_iter().for_each(|(key, value)| {
            if let (Ok(header_name), Ok(header_value)) = (
                HeaderName::from_bytes(key.as_bytes()),
                HeaderValue::from_bytes(value.as_bytes()),
            ) {
                header_map.insert(header_name, header_value);
            }
        });

        // if the content type is not provided - using the application/json by default
        if header_map.get(&CONTENT_TYPE).is_none() {
            header_map.insert(
                CONTENT_TYPE,
                HeaderValue::from_static(APPLICATION_JSON.as_ref())
            );
        }
        
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use hyper::StatusCode;
    use http_body_util::BodyExt;
    use serde::Serialize;
    use tokio::fs::File;
    use crate::{response, HttpResult, ResponseContext, Results};
    use crate::http::body::HttpBody;
    use crate::test_utils::read_file_bytes;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }
    
    #[tokio::test]
    async fn in_creates_text_response_with_custom_headers() {
        let mut headers = HashMap::new();
        headers.insert(String::from("x-api-key"), String::from("some api key"));
        
        let mut response = HttpResult::from(ResponseContext {
            status: 400,
            content: String::from("Hello World!"),
            headers
        }).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(String::from_utf8_lossy(body), "\"Hello World!\"");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[tokio::test]
    async fn in_creates_str_text_response_with_custom_headers() {
        let mut headers = HashMap::new();
        headers.insert(String::from("x-api-key"), String::from("some api key"));
        headers.insert(String::from("Content-Type"), String::from("text/plain"));

        let mut response = HttpResult::from(ResponseContext {
            status: 200,
            content: "Hello World!",
            headers,
        }).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body), "\"Hello World!\"");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[tokio::test]
    async fn in_creates_json_response_with_custom_headers() {
        let mut headers = HashMap::new();
        headers.insert(String::from("x-api-key"), String::from("some api key"));
        headers.insert(String::from("Content-Type"), String::from("application/json"));

        let content = TestPayload { name: "test".into() };
        
        let mut response = HttpResult::from(ResponseContext {
            status: 200,
            content,
            headers,
        }).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[tokio::test]
    async fn it_creates_json_response() {
        let payload = TestPayload { name: "test".into() };
        let mut response = Results::json(payload).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_json_response_with_custom_status() {
        let payload = TestPayload { name: "test".into() };
        let mut response = Results::json_with_status(StatusCode::NOT_FOUND, payload).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_text_response() {
        let mut response = Results::text("Hello World!").unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body), "Hello World!");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_stream_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file = File::open(path).await.unwrap();
        let body = HttpBody::file(file);
        
        let mut response = Results::stream(body.into_boxed()).unwrap();

        let body = read_file_bytes(&mut response).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
    }

    #[tokio::test]
    async fn it_creates_file_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        
        let file = File::open(path).await.unwrap();
        let mut response = Results::file(file_name, file).unwrap();

        let body = read_file_bytes(&mut response).await;
        
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
    }

    #[tokio::test]
    async fn it_creates_file_response_with_custom_headers() {
        let path = Path::new("tests/resources/test_file.txt");
        let file = File::open(path).await.unwrap();
        let mut response = response!(
            StatusCode::OK,
            HttpBody::file(file),
            [
                ("x-api-key", "some api key"),
                ("Content-Type", "application/octet-stream")
            ]
        ).unwrap();
        
        let body = read_file_bytes(&mut response).await;
        
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/octet-stream");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }
    
    #[tokio::test]
    async fn it_creates_empty_ok_response() {
        let mut response = Results::ok().unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_empty_not_found_response() {
        let mut response = response!(
            StatusCode::NOT_FOUND, 
            HttpBody::empty(),
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_client_closed_request_response() {
        let mut response = Results::client_closed_request().unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status().as_u16(), 499);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_empty_custom_response() {
        let mut response = response!(
            StatusCode::UNAUTHORIZED,
            HttpBody::empty(),
            [
                ("Content-Type", "application/pdf")
            ]
        ).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/pdf");
    }

    #[tokio::test]
    async fn it_creates_custom_response() {
        let mut response = response!(
            StatusCode::FORBIDDEN,
            HttpBody::full("Hello World!"),
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(String::from_utf8_lossy(body), "Hello World!");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }
    
    #[test]
    fn it_inserts_header() {
        let response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!"),
            [
                ("Content-Type", "text/plain")
            ]
        );
        
        let response = Results::with_header(response, "X-Api-Key", "some api key").unwrap();
        assert_eq!(response.headers().get("X-Api-Key").unwrap(), "some api key");
    }

    #[test]
    fn it_inserts_headers() {
        let response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!"),
            [
                ("Content-Type", "text/plain")
            ]
        );
        
        let response = Results::with_headers(response, [
            ("X-Api-Key", "some api key"),
            ("X-Api-Key-2", "some api key 2")
        ]).unwrap();
        
        assert_eq!(response.headers().get("X-Api-Key").unwrap(), "some api key");
        assert_eq!(response.headers().get("X-Api-Key-2").unwrap(), "some api key 2");   
    }
}
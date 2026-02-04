//! HTTP response utilities

use crate::error::Error;
use crate::http::{
    body::HttpBody,
    Extensions,
    StatusCode,
    Version
};

use hyper::{
    header::{
        HeaderMap,
        HeaderName,
        HeaderValue
    }, 
    body::{Body, SizeHint},
    http::response::Parts, 
    Response,
};

use crate::headers::{FromHeaders, Header};

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
        HttpResponseBuilder::new()
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

    /// Returns a typed HTTP header value
    #[inline]
    pub fn get_header<T: FromHeaders>(&self) -> Option<Header<T>> {
        self.headers()
            .get(T::NAME)
            .map(Header::from_ref)
    }

    /// Returns a view of all values associated with this HTTP header.
    #[inline]
    pub fn get_all_headers<T: FromHeaders>(&self) -> impl Iterator<Item = Header<T>> {
        self.headers()
            .get_all(T::NAME)
            .iter()
            .map(Header::from_ref)
    }

    /// Inserts the header into the response, replacing any existing values
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

    /// Attempts to insert the header into the response, replacing any existing
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

    /// Inserts the raw header into the response, replacing any existing values
    /// with the same header name.
    #[inline]
    pub fn insert_raw_header(&mut self, name: HeaderName, value: HeaderValue) {
        self.inner.headers_mut().insert(name, value);
    }

    /// Attempts to inserts the raw header into the response, replacing any existing values
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

#[cfg(test)]
#[allow(unreachable_pub)]
#[allow(unused)]
mod tests {
    use hyper::StatusCode;
    use http_body_util::BodyExt;
    use serde::Serialize;
    use tokio::fs::File;
    use crate::{response, HttpResponse};
    use crate::headers::{Header, HeaderValue, HeaderName, headers};
    use crate::http::body::HttpBody;
    use crate::test::TempFile;
    use crate::test::utils::read_file_bytes;
    
    headers! {
        (ApiKey, "x-api-key")
    }

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }
    
    #[tokio::test]
    async fn in_creates_text_response_with_custom_headers() {       
        let mut response = HttpResponse::builder()
            .status(400)
            .header_raw("x-api-key", "some api key")
            .header_raw("Content-Type", "text/plain")
            .body(HttpBody::full(String::from("Hello World!")))
            .unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(String::from_utf8_lossy(body), "Hello World!");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[tokio::test]
    async fn in_creates_str_text_response_with_custom_headers() {
        let mut response = HttpResponse::builder()
            .status(200)
            .header_raw("x-api-key", "some api key")
            .header_raw("Content-Type", "text/plain")
            .body(HttpBody::full("Hello World!"))
            .unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body), "Hello World!");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[tokio::test]
    async fn in_creates_json_response_with_custom_headers() {
        let content = TestPayload { name: "test".into() };
        
        let mut response = HttpResponse::builder()
            .status(200)
            .header_raw("x-api-key", "some api key")
            .header_raw("Content-Type", "application/json")
            .body(HttpBody::json(content).unwrap())
            .unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[tokio::test]
    async fn it_creates_stream_response() {
        let file = TempFile::new("Hello, this is some file content!").await;
        let file = File::open(file.path).await.unwrap();
        
        let mut response = HttpResponse::builder()
            .status(StatusCode::OK)
            .body(HttpBody::file(file))
            .unwrap();

        let body = read_file_bytes(&mut response).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
    }

    #[tokio::test]
    async fn it_creates_file_response_with_custom_headers() {
        let file = TempFile::new("Hello, this is some file content!").await;
        let file = File::open(file.path).await.unwrap();
        
        let mut response = response!(
            StatusCode::OK,
            HttpBody::file(file);
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
    async fn it_creates_empty_not_found_response() {
        let mut response = response!(
            StatusCode::NOT_FOUND, 
            HttpBody::empty();
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
    async fn it_creates_empty_custom_response() {
        let mut response = response!(
            StatusCode::UNAUTHORIZED,
            HttpBody::empty();
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
            HttpBody::full("Hello World!");
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
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        let api_key_header: Header<ApiKey> = Header::from_static("some api key");
        let _ = response.insert_header(api_key_header);

        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[test]
    fn it_tries_insert_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        response.try_insert_header::<ApiKey>("some api key").unwrap();

        assert_eq!(response.get_header::<ApiKey>().unwrap().value(), "some api key");  
    }

    #[test]
    fn it_inserts_raw_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        response.insert_raw_header(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("some api key"),
        );

        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
    }

    #[test]
    fn it_tries_insert_raw_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        response.try_insert_raw_header("x-api-key", "some api key").unwrap();

        assert_eq!(response.get_header::<ApiKey>().unwrap().value(), "some api key");  
    }

    #[test]
    fn it_appends_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        let api_key_header: Header<ApiKey> = Header::from_static("1");
        let _ = response.append_header(api_key_header);

        let api_key_header: Header<ApiKey> = Header::from_static("2");
        let _ = response.append_header(api_key_header);

        assert_eq!(response.headers().get_all("x-api-key").into_iter().collect::<Vec<_>>(), ["1", "2"]);
    }

    #[test]
    fn it_tries_append_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        response.try_append_header::<ApiKey>("1").unwrap();
        response.try_append_header::<ApiKey>("2").unwrap();

        assert_eq!(response.get_all_headers::<ApiKey>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]);  
    }

    #[test]
    fn it_appends_raw_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        response.append_raw_header(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("1"),
        );

        response.append_raw_header(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("2"),
        );

        assert_eq!(response.get_all_headers::<ApiKey>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]); 
    }

    #[test]
    fn it_tries_appends_raw_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        response.try_append_raw_header("x-api-key", "1").unwrap();
        response.try_append_raw_header("x-api-key", "2").unwrap();

        assert_eq!(response.get_all_headers::<ApiKey>().map(|h| h.into_inner()).collect::<Vec<_>>(), ["1", "2"]); 
    }

    #[test]
    fn it_removes_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        let api_key_header: Header<ApiKey> = Header::from_static("some api key");
        let _ = response.insert_header(api_key_header);

        response.remove_header::<ApiKey>();

        assert!(response.headers().get("x-api-key").is_none());
    }

    #[test]
    fn it_tries_remove_header() {
        let mut response = response!(
            StatusCode::OK,
            HttpBody::full("Hello World!");
            [
                ("Content-Type", "text/plain")
            ]
        ).unwrap();

        let api_key_header: Header<ApiKey> = Header::from_static("some api key");
        let _ = response.insert_header(api_key_header);

        let result = response.try_remove_header("x-api-key").unwrap();

        assert!(result);
        assert!(response.headers().get("x-api-key").is_none());
    }
}
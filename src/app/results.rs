﻿use std::collections::HashMap;
use tokio::fs::File;
use tokio::io;
use bytes::Bytes;
use chrono::Utc;
use serde::Serialize;
use http::{HeaderName, HeaderValue, Response, StatusCode};
use http::response::Builder;
use crate::app::body::{BoxBody, HttpBody};
use http::header::{ 
    CONTENT_DISPOSITION,
    CONTENT_LENGTH,
    CONTENT_TYPE,
    SERVER,
    DATE
};
use mime::{
    APPLICATION_OCTET_STREAM,
    APPLICATION_JSON,
    TEXT_PLAIN
};

pub mod macros;

/// A customized response context with custom response `headers` and `content_type`
/// > NOTE: This is not suitable for file response
/// # Example
/// ```no_run
///use volga::{App, AsyncEndpointsMapping, Results, ResponseContext};
///use std::collections::HashMap;
///
///#[tokio::main]
///async fn main() -> std::io::Result<()> {
///    let mut app = App::build("127.0.0.1:7878").await?;
///
///    app.map_get("/test", |req| async move {
///        let mut headers = HashMap::new();
///        headers.insert(String::from("x-api-key"), String::from("some api key"));
///        
///        Results::from(ResponseContext {
///            content: "Hello World!",
///            status: 200,
///            headers
///        })
///    });
///
///    app.run().await
///}
/// ```
pub struct ResponseContext<T: Serialize> {
    pub content: T,
    pub status: u16,
    pub headers: HttpHeaders
}

pub type HttpResponse = Response<BoxBody>;
pub type HttpResult = io::Result<HttpResponse>;
pub type HttpHeaders = HashMap<String, String>;

pub struct Results;

impl Results {
    /// Produces a customized `OK 200` response
    pub fn from<T: Serialize>(context: ResponseContext<T>) -> HttpResult {
        let ResponseContext { content, headers, status } = context;
        let content = serde_json::to_vec(&content)?;
        
        Self::create_custom_builder(headers)
            .status(StatusCode::from_u16(status).unwrap_or(StatusCode::OK))
            .header(CONTENT_LENGTH, content.len())
            .body(HttpBody::full(content))
            .map_err(|_| Self::response_error())
    }

    /// Produces an `OK 200` response with the `JSON` body.
    #[inline]
    pub fn json<T>(content: &T) -> HttpResult
    where T:
        ?Sized + Serialize
    {
        let content = serde_json::to_vec(content)?;
        let body = Bytes::from(content);
        Self::status(
            StatusCode::OK,
            APPLICATION_JSON.as_ref(),
            body)
    }

    /// Produces a response with `StatusCode` the `JSON` body.
    #[inline]
    pub fn json_with_status<T>(status: StatusCode, content: &T) -> HttpResult
    where T:
        ?Sized + Serialize
    {
        let content = serde_json::to_vec(content)?;
        let body = Bytes::from(content);
        Self::status(
            status,
            APPLICATION_JSON.as_ref(),
            body)
    }

    /// Produces an `OK 200` response with the plain text body.
    #[inline]
    pub fn text(content: &str) -> HttpResult {
        let body = Bytes::from(String::from(content));
        Self::status(
            StatusCode::OK,
            TEXT_PLAIN.as_ref(),
            body)
    }

    /// Produces an `OK 200` response with the file body.
    #[inline]
    pub async fn file(file_name: &str, content: File) -> HttpResult {
        let metadata = content.metadata().await?;
        let boxed_body = HttpBody::wrap_stream(content);
        
        let file_name = format!("attachment; filename=\"{}\"", file_name);
        Self::create_default_builder()
            .status(StatusCode::OK)
            .header(CONTENT_LENGTH, metadata.len())
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM.as_ref())
            .header(CONTENT_DISPOSITION, file_name)
            .body(boxed_body)
            .map_err(|_| Self::response_error())
    }

    /// Produces an `OK 200` response with the file body and custom headers.
    #[inline]
    pub async fn file_with_custom_headers(file_name: &str, content: File, mut headers: HttpHeaders) -> HttpResult {
        let metadata = content.metadata().await?;
        headers.insert(
            CONTENT_TYPE.as_str().into(),
            APPLICATION_OCTET_STREAM.as_ref().into()
        );

        let boxed_body = HttpBody::wrap_stream(content);
        
        let file_name = format!("attachment; filename=\"{}\"", file_name);
        Self::create_custom_builder(headers)
            .status(StatusCode::OK)
            .header(CONTENT_LENGTH, metadata.len())
            .header(CONTENT_DISPOSITION, file_name)
            .body(boxed_body)
            .map_err(|_| Self::response_error())
    }

    /// Produces an empty `OK 200` response.
    #[inline]
    pub fn ok() -> HttpResult {
        Self::status(
            StatusCode::OK,
            TEXT_PLAIN.as_ref(),
            Bytes::new())
    }

    /// Produces an `NOT FOUND 400` response.
    #[inline]
    pub fn not_found() -> HttpResult {
        Self::status(
            StatusCode::NOT_FOUND,
            TEXT_PLAIN.as_ref(),
            Bytes::new())
    }

    /// Produces an `INTERNAL SERVER ERROR 500` response.
    #[inline]
    pub fn internal_server_error(error: Option<String>) -> HttpResult {
        let body = Self::get_error_bytes(error);
        Self::status(
            StatusCode::INTERNAL_SERVER_ERROR,
            TEXT_PLAIN.as_ref(),
            body)
    }

    /// Produces an `BAD REQUEST 400` response.
    #[inline]
    pub fn bad_request(error: Option<String>) -> HttpResult {
        let body = Self::get_error_bytes(error);
        Self::status(
            StatusCode::BAD_REQUEST,
            TEXT_PLAIN.as_ref(),
            body)
    }

    /// Produces an `CLIENT CLOSED REQUEST 499` response.
    #[inline]
    pub fn client_closed_request() -> HttpResult {
        Self::status(
            StatusCode::from_u16(499).unwrap(),
            TEXT_PLAIN.as_ref(),
            Bytes::new())
    }

    #[inline]
    pub fn status(status: StatusCode, content_type: &str, content: Bytes) -> HttpResult {
        Self::create_default_builder()
            .status(status)
            .header(CONTENT_LENGTH, content.len())
            .header(CONTENT_TYPE, content_type)
            .body(HttpBody::create(content))
            .map_err(|_| Self::response_error())
    }

    #[inline]
    fn create_default_builder() -> Builder {
        Response::builder()
            .header(DATE, Utc::now().to_rfc2822())
            .header(SERVER, "Volga")
    }

    #[inline]
    fn create_custom_builder(headers: HttpHeaders) -> Builder {
        let mut builder = Self::create_default_builder();

        if let Some(headers_ref) = builder.headers_mut() {
            for (name, value) in headers {
                let header_name = HeaderName::from_bytes(name.as_bytes());
                let header_value = HeaderValue::from_bytes(value.as_bytes());

                match (header_name, header_value) {
                    (Ok(header_name), Ok(header_value)) => headers_ref.insert(header_name, header_value),
                    _ => None
                };
            }
            // if the content type is not provided - using the application/json by default
            if headers_ref.get(CONTENT_TYPE).is_none() {
                headers_ref.insert(CONTENT_TYPE, HeaderValue::from_bytes(APPLICATION_JSON.as_ref().as_bytes()).unwrap());
            }
        } else if cfg!(debug_assertions) {
            eprintln!("Failed to write to HTTP headers");
        }

        builder
    }
    
    #[inline]
    fn response_error() -> io::Error {
        io::Error::new(io::ErrorKind::Other, "Unable to create a response")
    }

    #[inline]
    fn get_error_bytes(error: Option<String>) -> Bytes {
        if let Some(error) = error {
            Bytes::from(error)
        } else {
            Bytes::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use bytes::Bytes;
    use http::StatusCode;
    use http_body_util::BodyExt;
    use serde::Serialize;
    use tokio::fs::File;
    use crate::{headers, ResponseContext, Results};
    use crate::test_utils::read_file_bytes;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }
    
    #[tokio::test]
    async fn in_creates_text_response_with_custom_headers() {
        let mut headers = HashMap::new();
        headers.insert(String::from("x-api-key"), String::from("some api key"));
        
        let mut response = Results::from(ResponseContext {
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

        let mut response = Results::from(ResponseContext {
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
        
        let mut response = Results::from(ResponseContext {
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
        let mut response = Results::json(&payload).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_json_response_with_custom_status() {
        let payload = TestPayload { name: "test".into() };
        let mut response = Results::json_with_status(StatusCode::NOT_FOUND, &payload).unwrap();

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
    async fn it_creates_file_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        
        let file = File::open(path).await.unwrap();
        let mut response = Results::file(file_name, file).await.unwrap();

        let body = read_file_bytes(&mut response).await;
        
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/octet-stream");
    }

    #[tokio::test]
    async fn it_creates_file_response_with_custom_headers() {
        let path = Path::new("tests/resources/test_file.txt");
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();

        let headers = headers![
            ("x-api-key", "some api key")
        ];
        
        let file = File::open(path).await.unwrap();
        let mut response = Results::file_with_custom_headers(file_name, file, headers).await.unwrap();

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
        let mut response = Results::not_found().unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_empty_internal_server_error_response() {
        let mut response = Results::internal_server_error(None).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_internal_server_error_response() {
        let mut response = Results::internal_server_error(Some("Some error".into())).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(String::from_utf8_lossy(body), "Some error");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_empty_bad_request_response() {
        let mut response = Results::bad_request(None).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }

    #[tokio::test]
    async fn it_creates_bad_request_response() {
        let mut response = Results::bad_request(Some("Some error".into())).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(String::from_utf8_lossy(body), "Some error");
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
        let mut response = Results::status(StatusCode::UNAUTHORIZED, mime::APPLICATION_PDF.as_ref(), Bytes::new()).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/pdf");
    }

    #[tokio::test]
    async fn it_creates_custom_response() {
        let mut response = Results::status(
            StatusCode::FORBIDDEN,
            mime::TEXT_PLAIN.as_ref(), 
            Bytes::from(String::from("Hello World!"))).unwrap();

        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(String::from_utf8_lossy(body), "Hello World!");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
    }
}
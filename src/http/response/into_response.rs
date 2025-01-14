use super::{HttpResponse, HttpResult, HttpBody};
use crate::{Json, Form, ok, status, form, response};
use crate::http::StatusCode;
use crate::headers::CONTENT_TYPE;
use mime::TEXT_PLAIN_UTF_8;

use std::{
    io::{Error, ErrorKind::InvalidInput},
    convert::Infallible,
};
use serde::Serialize;

/// Trait for types that can be returned from request handlers
pub trait IntoResponse {
    fn into_response(self) -> HttpResult;
}

impl IntoResponse for HttpResponse {
    #[inline]
    fn into_response(self) -> HttpResult {
        Ok(self)
    }
}

impl IntoResponse for () {
    #[inline]
    fn into_response(self) -> HttpResult {
        ok!()
    }
}

impl IntoResponse for Error {
    #[inline]
    fn into_response(self) -> HttpResult {
        if self.kind() == InvalidInput {
            status!(400, self.to_string())
        } else {
            status!(500, self.to_string())
        } 
    }
}

impl IntoResponse for Infallible {
    #[inline]
    fn into_response(self) -> HttpResult {
        match self {}
    }
}

impl<T, E> IntoResponse for Result<T, E>
where
    T: IntoResponse,
    E: IntoResponse
{
    #[inline]
    fn into_response(self) -> HttpResult {
        match self { 
            Ok(ok) => ok.into_response(),
            Err(err) => err.into_response(),
        }
    }
}

impl IntoResponse for &'static str {
    #[inline]
    fn into_response(self) -> HttpResult {
        response!(
            StatusCode::OK,
            HttpBody::full(self),
            [(CONTENT_TYPE, TEXT_PLAIN_UTF_8.as_ref())]
        )
    }
}

impl IntoResponse for String {
    #[inline]
    fn into_response(self) -> HttpResult {
        response!(
            StatusCode::OK,
            HttpBody::full(self),
            [(CONTENT_TYPE, TEXT_PLAIN_UTF_8.as_ref())]
        )
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> HttpResult {
        ok!(self.into_inner())
    }
}

impl<T: Serialize> IntoResponse for Form<T> {
    fn into_response(self) -> HttpResult {
        form!(self.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind};
    use http_body_util::BodyExt;
    use serde::Serialize;
    use super::IntoResponse;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }
    
    #[tokio::test]
    async fn it_converts_into_response() {
        let response = ().into_response();
        
        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_str_into_response() {
        let response = "test".into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "test");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_string_into_response() {
        let response = String::from("test").into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "test");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_ok_result_into_response() {
        let response = Result::<&str, Error>::Ok("test").into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "test");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_input_err_result_into_response() {
        let response = Result::<&str, Error>::Err(Error::new(ErrorKind::InvalidInput, "some error")).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"some error\"");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn it_converts_err_result_into_response() {
        let response = Result::<&str, Error>::Err(Error::new(ErrorKind::InvalidData, "some error")).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"some error\"");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.status(), 500);
    }

    #[tokio::test]
    async fn it_converts_response_into_response() {
        let response = crate::ok!("test").unwrap().into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_json_into_response() {
        let payload = TestPayload { name: "test".into() };
        let response = crate::Json(payload).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_form_into_response() {
        let payload = TestPayload { name: "test".into() };
        let response = crate::Form(payload).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "name=test");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/x-www-form-urlencoded");
        assert_eq!(response.status(), 200);
    }
}

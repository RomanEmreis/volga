//! [`From ] trait implementations from various types into HTTP response

use super::{HttpResponse, HttpResult, HttpBody};
use crate::{Json, Form, ok, status, form, response};
use crate::error::Error;
use crate::http::StatusCode;
use crate::headers::{HeaderMap, CONTENT_TYPE};
use mime::TEXT_PLAIN_UTF_8;
use serde::Serialize;

#[cfg(feature = "cookie")]
use crate::http::{Cookies, cookie::set_cookies};
#[cfg(feature = "signed-cookie")]
use crate::http::SignedCookies;
#[cfg(feature = "private-cookie")]
use crate::http::PrivateCookies;

use std::{
    io::Error as IoError,
    convert::Infallible,
    borrow::Cow
};

/// Trait for types that can be returned from request handlers
pub trait IntoResponse {
    /// Converts object into response
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

impl IntoResponse for IoError {
    #[inline]
    fn into_response(self) -> HttpResult {
        Err(self.into())
    }
}

impl IntoResponse for Error {
    #[inline]
    fn into_response(self) -> HttpResult {
        Err(self)
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
        Cow::Borrowed(self).into_response()
    }
}

impl IntoResponse for Cow<'static, str> {
    #[inline]
    fn into_response(self) -> HttpResult {
        response!(
            StatusCode::OK,
            HttpBody::from(self),
            [(CONTENT_TYPE, TEXT_PLAIN_UTF_8.as_ref())]
        )
    }
}

impl IntoResponse for String {
    #[inline]
    fn into_response(self) -> HttpResult {
        Cow::<'static, str>::Owned(self).into_response()
    }
}

impl IntoResponse for Box<str> {
    #[inline]
    fn into_response(self) -> HttpResult {
        String::from(self).into_response()
    }
}

impl<T: IntoResponse> IntoResponse for Option<T> {
    #[inline]
    fn into_response(self) -> HttpResult {
        match self { 
            Some(ok) => ok.into_response(),
            None => status!(404)
        }
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    #[inline]
    fn into_response(self) -> HttpResult {
        ok!(self.into_inner())
    }
}

impl<T: Serialize> IntoResponse for Form<T> {
    #[inline]
    fn into_response(self) -> HttpResult {
        form!(self.into_inner())
    }
}

impl IntoResponse for StatusCode {
    #[inline]
    fn into_response(self) -> HttpResult {
        response!(
            self,
            HttpBody::empty()
        )
    }
}

impl<R> IntoResponse for (R, HeaderMap)
where 
    R: IntoResponse
{
    #[inline]
    fn into_response(self) -> HttpResult {
        let (resp, headers) = self;
        match resp.into_response() {
            Err(err) => Err(err),
            Ok(mut resp) => { 
                resp.headers_mut().extend(headers);
                Ok(resp)
            },
        }
    }
}

#[cfg(feature = "signed-cookie")]
impl<R> IntoResponse for (R, SignedCookies)
where 
    R: IntoResponse
{
    #[inline]
    fn into_response(self) -> HttpResult {
        let (resp, cookies) = self;
        match resp.into_response() {
            Err(err) => Err(err),
            Ok(mut resp) => {
                let (_, jar) = cookies.into_parts();
                set_cookies(jar, resp.headers_mut());
                Ok(resp)
            },
        }
    }    
}

#[cfg(feature = "private-cookie")]
impl<R> IntoResponse for (R, PrivateCookies)
where
    R: IntoResponse
{
    #[inline]
    fn into_response(self) -> HttpResult {
        let (resp, cookies) = self;
        match resp.into_response() {
            Err(err) => Err(err),
            Ok(mut resp) => {
                let (_, jar) = cookies.into_parts();
                set_cookies(jar, resp.headers_mut());
                Ok(resp)
            },
        }
    }
}

#[cfg(feature = "cookie")]
impl<R> IntoResponse for (R, Cookies)
where
    R: IntoResponse
{
    #[inline]
    fn into_response(self) -> HttpResult {
        let (resp, cookies) = self;
        match resp.into_response() {
            Err(err) => Err(err),
            Ok(mut resp) => {
                set_cookies(cookies.into_inner(), resp.headers_mut());
                Ok(resp)
            },
        }
    }
}

macro_rules! impl_into_response {
    { $($type:ident),* $(,)? } => {
        $(impl IntoResponse for $type {
            #[inline]
            fn into_response(self) -> HttpResult {
                response!(
                    $crate::http::StatusCode::OK,
                    HttpBody::full(self.to_string()),
                    [($crate::headers::CONTENT_TYPE, "text/plain; charset=utf-8")]
                )
            }
        })*
    };
}

impl_into_response! {
    bool,
    i8, u8,
    i16, u16,
    i32, u32,
    f32,
    i64, u64,
    f64,
    i128, u128,
    isize, usize
}

#[cfg(test)]
mod tests {
    use std::io::{Error as IoError, ErrorKind};
    use http_body_util::BodyExt;
    use hyper::StatusCode;
    use serde::Serialize;
    use crate::error::Error;
    use crate::headers::HeaderMap;
    use super::IntoResponse;
    #[cfg(feature = "cookie")]
    use crate::http::Cookies;

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
        assert!(response.headers().get("Content-Type").is_none());
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
    async fn it_converts_err_into_response() {
        let response = IoError::new(ErrorKind::InvalidInput, "some error").into_response();

        assert!(response.is_err());
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
    async fn it_converts_err_result_into_response() {
        let response = Result::<&str, IoError>::Err(IoError::new(ErrorKind::InvalidInput, "some error")).into_response();

        assert!(response.is_err());
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

    #[tokio::test]
    async fn it_converts_option_some_into_response() {
        let response = Some("test").into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "test");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_option_none_into_response() {
        let response = Option::<&str>::None.into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert!(response.headers().get("Content-Type").is_none());
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_converts_box_str_into_response() {
        let response = String::from("test").into_boxed_str().into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "test");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[test]
    fn it_converts_error_into_response() {
        let response = Error::server_error("some error").into_response();

        assert!(response.is_err());
    }

    #[tokio::test]
    async fn it_converts_int_into_response() {
        let response = (-7878i32).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "-7878");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_float_into_response() {
        let response = 1.25f32.into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "1.25");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_uint_into_response() {
        let response = 7878u32.into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "7878");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_bool_into_response() {
        let response = true.into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "true");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/plain; charset=utf-8");
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_converts_status_response() {
        let response = StatusCode::SEE_OTHER.into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 303);
    }

    #[tokio::test]
    async fn it_converts_tuple_of_status_and_headers_into_response() {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "some api key".parse().unwrap());
        headers.insert("x-api-secret", "some api secret".parse().unwrap());
        
        let response = (
            StatusCode::NO_CONTENT,
            headers
        ).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.status(), 204);
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-api-secret").unwrap(), "some api secret");
    }
    
    #[tokio::test]
    #[cfg(feature = "cookie")]
    async fn it_converts_tuple_of_redirect_status_and_cookies_into_redirect_response() {
        let mut cookies = Cookies::new();
        cookies = cookies
            .add(("key-1", "value-1"))
            .add(("key-2", "value-2"));
        
        let response = (
            crate::found!("https://www.rust-lang.org/"), 
            cookies
        ).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("location").unwrap(), "https://www.rust-lang.org/");
        assert_eq!(response.status(), 302);

        let cookies = get_cookies(response.headers()); 
        
        assert!(cookies.contains(&"key-1=value-1"));
        assert!(cookies.contains(&"key-2=value-2"));
    }

    #[tokio::test]
    #[cfg(feature = "signed-cookie")]
    async fn it_converts_tuple_of_redirect_status_and_signed_cookies_into_redirect_response() {
        use crate::http::{SignedKey, SignedCookies};
        
        let key = SignedKey::generate();
        let mut cookies = SignedCookies::new(key);
        cookies = cookies
            .add(("key-1", "value-1"))
            .add(("key-2", "value-2"));

        let response = (
            crate::see_other!("https://www.rust-lang.org/"),
            cookies
        ).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("location").unwrap(), "https://www.rust-lang.org/");
        assert_eq!(response.status(), 303);

        let cookies = get_cookies(response.headers());

        assert_eq!(cookies.iter().filter(|c| c.contains("key-1")).count(), 1);
        assert_eq!(cookies.iter().filter(|c| c.contains("key-2")).count(), 1);
    }

    #[tokio::test]
    #[cfg(feature = "private-cookie")]
    async fn it_converts_tuple_of_redirect_status_and_private_cookies_into_redirect_response() {
        use crate::http::{PrivateKey, PrivateCookies};

        let key = PrivateKey::generate();
        let mut cookies = PrivateCookies::new(key);
        cookies = cookies
            .add(("key-1", "value-1"))
            .add(("key-2", "value-2"));

        let response = (
            crate::see_other!("https://www.rust-lang.org/"),
            cookies
        ).into_response();

        assert!(response.is_ok());
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(body.len(), 0);
        assert_eq!(response.headers().get("location").unwrap(), "https://www.rust-lang.org/");
        assert_eq!(response.status(), 303);

        let cookies = get_cookies(response.headers());

        assert_eq!(cookies.iter().filter(|c| c.contains("key-1")).count(), 1);
        assert_eq!(cookies.iter().filter(|c| c.contains("key-2")).count(), 1);
    }

    #[cfg(any(
        feature = "private-cookie",
        feature = "signed-cookie",
        feature = "cookie",
    ))]
    fn get_cookies(headers: &HeaderMap) -> Vec<&str> {
        headers
            .get_all("set-cookie")
            .iter()
            .map(|cookie| cookie.to_str().unwrap())
            .collect::<Vec<&str>>()
    }
}

//! Set of utils to work with Cookies

use std::ops::{Deref, DerefMut};
use cookie::CookieJar;
use futures_util::future::{ready, Ready};
use crate::{
    error::Error,
    headers::{COOKIE, SET_COOKIE, HeaderMap}, 
    http::{endpoints::args::{FromPayload, Payload, Source}},
};

/// Represents HTTP cookies
#[derive(Debug, Default, Clone)]
pub struct Cookies(CookieJar);

impl Deref for Cookies {
    type Target = CookieJar;
    
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Cookies {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<&HeaderMap> for Cookies {
    #[inline]
    fn from(headers: &HeaderMap) -> Self {
        let mut jar = CookieJar::new();
        let cookies = headers
            .get_all(COOKIE)
            .into_iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|cookie| cookie::Cookie::parse_encoded(cookie.to_owned()).ok());

        for cookie in cookies {
            jar.add_original(cookie);
        }
        
        Self(jar)
    }
}

impl From<Cookies> for HeaderMap {
    #[inline]
    fn from(cookies: Cookies) -> Self {
        let mut headers = Self::new();
        cookies.set_cookies(&mut headers);
        headers
    }
}

impl Cookies {
    /// Creates a new [`Cookies`]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Sets cookies to the HTTP headers
    #[inline]
    pub(crate) fn set_cookies(self, headers: &mut HeaderMap) {
        for cookie in self.delta() {
            if let Ok(header_value) = cookie.encoded().to_string().parse() {
                headers.append(SET_COOKIE, header_value);
            }
        }
    }
}

impl FromPayload for Cookies {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Ok(Cookies::from(&parts.headers)))
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::HeaderValue;

    #[test]
    fn it_creates_cookies_from_empty_headers() {
        let headers = HeaderMap::new();
        let cookies = Cookies::from(&headers);
        assert_eq!(cookies.iter().count(), 0);
    }

    #[test]
    fn it_creates_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("session=abc123"),
        );

        let cookies = Cookies::from(&headers);
        let cookie = cookies.get("session").expect("Cookie should exist");
        assert_eq!(cookie.value(), "abc123");
    }

    #[test]
    fn it_creates_from_multiple_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("session=abc123; user=john; theme=dark"),
        );

        let cookies = Cookies::from(&headers);
        assert_eq!(cookies.get("session").unwrap().value(), "abc123");
        assert_eq!(cookies.get("user").unwrap().value(), "john");
        assert_eq!(cookies.get("theme").unwrap().value(), "dark");
    }

    #[test]
    fn it_removes_cookies() {
        let mut cookies = Cookies::default();

        // Add a new cookie
        cookies.add(cookie::Cookie::new("test", "value"));
        assert_eq!(cookies.get("test").unwrap().value(), "value");

        // Remove a cookie
        cookies.remove(cookie::Cookie::new("test", ""));
        assert!(cookies.get("test").is_none());
    }

    #[test]
    fn it_sets_cookies_to_headers() {
        let mut cookies = Cookies::default();
        cookies.add(cookie::Cookie::new("session", "xyz789"));

        let mut headers = HeaderMap::new();
        cookies.set_cookies(&mut headers);

        let cookie_header = headers.get(SET_COOKIE).expect("Cookie header should be set");
        assert!(cookie_header.to_str().unwrap().contains("session=xyz789"));
    }

    #[tokio::test]
    async fn it_extracts_from_payload() {
        use hyper::Request;

        let request = Request::builder()
            .header(COOKIE, "test=value")
            .body(())
            .unwrap();

        let (parts, _) = request.into_parts();
        let payload = Payload::Parts(&parts);

        let cookies = Cookies::from_payload(payload).await.unwrap();

        assert_eq!(cookies.get("test").unwrap().value(), "value");
    }

    #[test]
    fn test_source() {
        assert_eq!(Cookies::source(), Source::Parts);
    }

    #[test]
    fn test_deref_and_deref_mut() {
        let mut cookies = Cookies::default();

        // Test Deref
        cookies.add(cookie::Cookie::new("test", "value"));
        assert_eq!(cookies.deref().get("test").unwrap().value(), "value");

        // Test DerefMut
        cookies.deref_mut().add(cookie::Cookie::new("test2", "value2"));
        assert_eq!(cookies.get("test2").unwrap().value(), "value2");
    }
}
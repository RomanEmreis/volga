//! Set of utils to work with Cookies

use cookie::CookieJar;
use futures_util::future::{ready, Ready};
use crate::{
    error::Error, 
    headers::{COOKIE, SET_COOKIE, HeaderMap}, 
    http::{
        HttpRequest, Request,
        body::Incoming,
        endpoints::args::{
        FromRequestRef,
        FromRequestParts,
        FromRawRequest,
        FromPayload,
        Payload,
        Source
    }},
};
use crate::http::Parts;

#[cfg(feature = "signed-cookie")]
pub mod signed;
#[cfg(feature = "private-cookie")]
pub mod private;

/// Represents HTTP cookies
#[derive(Debug, Default, Clone)]
pub struct Cookies(CookieJar);

impl From<&HeaderMap> for Cookies {
    #[inline]
    fn from(headers: &HeaderMap) -> Self {
        let mut jar = CookieJar::new();
        for cookie in get_cookies(headers) {
            jar.add_original(cookie);
        }
        
        Self(jar)
    }
}

impl From<Cookies> for HeaderMap {
    #[inline]
    fn from(cookies: Cookies) -> Self {
        let mut headers = Self::new();
        set_cookies(cookies.0, &mut headers);
        headers
    }
}

impl Cookies {
    /// Creates a new [`Cookies`]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Unwraps the inner jar
    #[inline]
    pub fn into_inner(self) -> CookieJar {
        self.0
    }

    /// Returns a reference to the cookie inside the jar by `name`
    /// If the cookie cannot be found, `None` is returned.
    pub fn get(&self, name: &str) -> Option<&cookie::Cookie<'static>> {
        self.0.get(name)
    }

    /// Adds a cookie. If a cookie with the same name already exists, it is replaced with this cookie.
    #[allow(clippy::should_implement_trait)]
    pub fn add<C: Into<cookie::Cookie<'static>>>(mut self, cookie: C) -> Self {
        self.0.add(cookie);
        self
    }

    /// Removes cookie from this jar. If an original cookie with the same name as the cookie is present in the jar,
    /// a removal cookie will be present in the delta computation.
    ///
    /// To properly generate the removal cookie, this cookie must contain the same path and domain as the cookie that was initially set.
    pub fn remove<C: Into<cookie::Cookie<'static>>>(mut self, cookie: C) -> Self {
        self.0.remove(cookie);
        self
    }

    /// Returns an iterator over all the cookies present in this jar.
    pub fn iter(&self) -> impl Iterator<Item = &cookie::Cookie<'static>> + '_ {
        self.0.iter()
    }
}

/// Gets cookies from HTTP request's [`HeaderMap`]
#[inline]
fn get_cookies(headers: &HeaderMap) -> impl Iterator<Item = cookie::Cookie<'static>> + '_ {
    headers
        .get_all(COOKIE)
        .into_iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|cookie| cookie::Cookie::parse_encoded(cookie.to_owned()).ok())
}

/// Sets cookies to the HTTP headers
#[inline]
pub(crate) fn set_cookies(jar: CookieJar, headers: &mut HeaderMap) {
    for cookie in jar.delta() {
        if let Ok(header_value) = cookie.encoded().to_string().parse() {
            headers.append(SET_COOKIE, header_value);
        }
    }
}

impl FromRequestRef for Cookies {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Ok(Cookies::from(req.headers()))
    }
}

impl FromRequestParts for Cookies {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(Cookies::from(&parts.headers))
    }
}

impl FromRawRequest for Cookies {
    #[inline]
    fn from_request(req: Request<Incoming>) -> impl Future<Output = Result<Self, Error>> + Send {
        ready(Ok(Cookies::from(req.headers())))
    }
}

impl FromPayload for Cookies {
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
    use super::*;
    use crate::headers::HeaderValue;
    use hyper::Request;
    use crate::HttpBody;

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
        cookies = cookies.add(cookie::Cookie::new("test", "value"));
        assert_eq!(cookies.get("test").unwrap().value(), "value");

        // Remove a cookie
        cookies = cookies.remove(cookie::Cookie::new("test", ""));
        assert!(cookies.get("test").is_none());
    }

    #[test]
    fn it_sets_cookies_to_headers() {
        let mut cookies = Cookies::default();
        cookies = cookies.add(cookie::Cookie::new("session", "xyz789"));

        let mut headers = HeaderMap::new();
        set_cookies(cookies.0, &mut headers);

        let cookie_header = headers.get(SET_COOKIE).expect("Cookie header should be set");
        assert!(cookie_header.to_str().unwrap().contains("session=xyz789"));
    }

    #[tokio::test]
    async fn it_extracts_from_payload() {
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
    fn it_extracts_from_parts() {
        let request = Request::builder()
            .header(COOKIE, "test=value")
            .body(())
            .unwrap();

        let (parts, _) = request.into_parts();
        
        let cookies = Cookies::from_parts(&parts).unwrap();

        assert_eq!(cookies.get("test").unwrap().value(), "value");
    }

    #[test]
    fn it_extracts_from_request_ref() {
        let request = Request::builder()
            .header(COOKIE, "test=value")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = request.into_parts();
        let request = HttpRequest::from_parts(parts, body);

        let cookies = <Cookies as FromRequestRef>::from_request(&request).unwrap();

        assert_eq!(cookies.get("test").unwrap().value(), "value");
    }

    #[test]
    fn it_returns_parts_source() {
        assert_eq!(Cookies::SOURCE, Source::Parts);
    }
}
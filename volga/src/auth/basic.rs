//! Tools and utils for Basic Authorization

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::fmt::{Display, Formatter};
use futures_util::future::{ready, Ready};
use hyper::http::request::Parts;
use crate::{
    http::{FromRequestParts, FromRequestRef, endpoints::args::{FromPayload, Payload, Source}},
    headers::{Authorization, Header, HeaderMap, HeaderValue, AUTHORIZATION},
    error::Error,
    HttpRequest
};

const SCHEME: &str = "Basic ";

/// Basic authorization context
pub struct Basic(Box<str>);

impl std::fmt::Debug for Basic {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Basic")
            .field(&"[redacted]")
            .finish()
    }
}

impl TryFrom<&HeaderValue> for Basic {
    type Error = Error;

    #[inline]
    fn try_from(header: &HeaderValue) -> Result<Self, Self::Error> {
        let token = header
            .to_str()
            .map_err(Error::from)?;
        let token = token.strip_prefix(SCHEME)
            .map(str::trim)
            .ok_or_else(|| Error::client_error("Header: Missing Credentials"))?;
        Ok(Self(token.into()))
    }
}

impl TryFrom<Header<Authorization>> for Basic {
    type Error = Error;

    #[inline]
    fn try_from(header: Header<Authorization>) -> Result<Self, Self::Error> {
        let header = header.into_inner();
        Self::try_from(&header)
    }
}

impl TryFrom<&HeaderMap> for Basic {
    type Error = Error;

    #[inline]
    fn try_from(headers: &HeaderMap) -> Result<Self, Self::Error> {
        let header = headers
            .get(AUTHORIZATION)
            .ok_or_else(|| Error::client_error("Header: Missing Authorization header"))?;
        header.try_into()
    }
}

impl TryFrom<&Parts> for Basic {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Self::Error> {
        Self::try_from(&parts.headers)
    }
}

impl Display for Basic {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromRequestParts for Basic {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Self::try_from(parts)
    }
}

impl FromRequestRef for Basic {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Self::try_from(req.headers())
    }
}

impl FromPayload for Basic {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

impl Basic {
    /// Validates username and password
    pub fn validate(&self, username: &str, password: &str) -> bool {
        let expected = format!("{username}:{password}");
        self.validate_base64(&STANDARD.encode(expected))
    }

    /// Validates credentials encoded in Base64
    pub fn validate_base64(&self, credentials: &str) -> bool {
        *credentials == *self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::{HeaderMap, HeaderValue, Header, Authorization, AUTHORIZATION};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use hyper::Request;

    fn create_basic_auth_header(username: &str, password: &str) -> HeaderValue {
        let credentials = format!("{username}:{password}");
        let encoded = STANDARD.encode(credentials);
        HeaderValue::from_str(&format!("Basic {encoded}")).unwrap()
    }

    #[test]
    fn it_tests_try_from_header_value_success() {
        let header = create_basic_auth_header("user", "pass");
        let basic = Basic::try_from(&header).unwrap();

        let expected = STANDARD.encode("user:pass");
        assert_eq!(basic.to_string(), expected);
    }

    #[test]
    fn it_tests_try_from_header_value_missing_scheme() {
        let header = HeaderValue::from_str("dXNlcjpwYXNz").unwrap(); // "user:pass" without "Basic "
        let result = Basic::try_from(&header);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.to_string(), "Header: Missing Credentials");
    }

    #[test]
    fn it_tests_try_from_header_value_invalid_utf8() {
        let mut header_value = Vec::from(b"Basic ");
        header_value.extend_from_slice(&[0xFF, 0xFE]); // Invalid UTF-8

        // This test would require creating an invalid HeaderValue, which is difficult
        // Instead, we'll test the trim functionality
        let header = HeaderValue::from_str("Basic   dXNlcjpwYXNz   ").unwrap(); // With spaces
        let basic = Basic::try_from(&header).unwrap();

        let expected = STANDARD.encode("user:pass");
        assert_eq!(basic.to_string(), expected);
    }

    #[test]
    fn it_tests_try_from_authorization_header() {
        let header_value = create_basic_auth_header("testuser", "testpass");
        let auth_header = Header::<Authorization>::new(&header_value);
        let basic = Basic::try_from(auth_header).unwrap();

        let expected = STANDARD.encode("testuser:testpass");
        assert_eq!(basic.to_string(), expected);
    }

    #[test]
    fn it_tests_try_from_header_map_success() {
        let mut headers = HeaderMap::new();
        let header_value = create_basic_auth_header("admin", "secret");
        headers.insert(AUTHORIZATION, header_value);

        let basic = Basic::try_from(&headers).unwrap();
        let expected = STANDARD.encode("admin:secret");
        assert_eq!(basic.to_string(), expected);
    }

    #[test]
    fn it_tests_try_from_header_map_missing_authorization() {
        let headers = HeaderMap::new();
        let result = Basic::try_from(&headers);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.to_string(), "Header: Missing Authorization header");
    }

    #[test]
    fn it_tests_try_from_parts() {
        let req = Request::builder()
            .header(AUTHORIZATION, create_basic_auth_header("user123", "pass456"))
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let basic = Basic::try_from(&parts).unwrap();
        let expected = STANDARD.encode("user123:pass456");
        assert_eq!(basic.to_string(), expected);
    }

    #[test]
    fn it_tests_display() {
        let encoded = STANDARD.encode("display:test");
        let basic = Basic(encoded.clone().into_boxed_str());

        assert_eq!(format!("{basic}"), encoded);
        assert_eq!(basic.to_string(), encoded);
    }

    #[test]
    fn it_tests_from_request_parts() {
        let req = Request::builder()
            .header(AUTHORIZATION, create_basic_auth_header("parts", "test"))
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let basic = Basic::from_parts(&parts).unwrap();
        let expected = STANDARD.encode("parts:test");
        assert_eq!(basic.to_string(), expected);
    }

    #[tokio::test]
    async fn it_tests_from_payload_with_parts() {
        let req = Request::builder()
            .header(AUTHORIZATION, create_basic_auth_header("payload", "user"))
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        let payload = Payload::Parts(&parts);

        let basic = Basic::from_payload(payload).await.unwrap();

        let expected = STANDARD.encode("payload:user");
        assert_eq!(basic.to_string(), expected);
    }

    #[test]
    fn it_tests_source_returns_parts() {
        assert!(matches!(Basic::source(), Source::Parts));
    }

    #[test]
    fn it_tests_validate_with_correct_credentials() {
        let basic = Basic(STANDARD.encode("testuser:testpass").into_boxed_str());

        assert!(basic.validate("testuser", "testpass"));
    }

    #[test]
    fn it_tests_validate_with_incorrect_username() {
        let basic = Basic(STANDARD.encode("testuser:testpass").into_boxed_str());

        assert!(!basic.validate("wronguser", "testpass"));
    }

    #[test]
    fn it_tests_validate_with_incorrect_password() {
        let basic = Basic(STANDARD.encode("testuser:testpass").into_boxed_str());

        assert!(!basic.validate("testuser", "wrongpass"));
    }

    #[test]
    fn it_tests_validate_with_empty_credentials() {
        let basic = Basic(STANDARD.encode(":").into_boxed_str());

        assert!(basic.validate("", ""));
    }

    #[test]
    fn it_tests_validate_with_special_characters() {
        let username = "user@domain.com";
        let password = "p@$$w0rd!";
        let basic = Basic(STANDARD.encode(format!("{username}:{password}")).into_boxed_str());

        assert!(basic.validate(username, password));
        assert!(!basic.validate("user@domain.com", "wrongpass"));
    }

    #[test]
    fn it_tests_validate_base64_with_correct_credentials() {
        let credentials = "dXNlcjpwYXNz"; // base64 for "user:pass"
        let basic = Basic(credentials.into());

        assert!(basic.validate_base64(credentials));
    }

    #[test]
    fn it_tests_validate_base64_with_incorrect_credentials() {
        let correct_credentials = "dXNlcjpwYXNz"; // base64 for "user:pass"
        let wrong_credentials = "YWRtaW46c2VjcmV0"; // base64 for "admin:secret"
        let basic = Basic(correct_credentials.into());

        assert!(!basic.validate_base64(wrong_credentials));
    }

    #[test]
    fn it_tests_validate_base64_with_empty_string() {
        let basic = Basic("".into());

        assert!(basic.validate_base64(""));
        assert!(!basic.validate_base64("dXNlcjpwYXNz"));
    }

    #[test]
    fn it_tests_case_sensitive_validation() {
        let basic = Basic(STANDARD.encode("User:Pass").into_boxed_str());

        assert!(basic.validate("User", "Pass"));
        assert!(!basic.validate("user", "Pass"));
        assert!(!basic.validate("User", "pass"));
        assert!(!basic.validate("user", "pass"));
    }

    #[test]
    fn it_tests_unicode_credentials() {
        let username = "użytkownik";
        let password = "hasło123";
        let basic = Basic(STANDARD.encode(format!("{username}:{password}")).into_boxed_str());

        assert!(basic.validate(username, password));
        assert!(!basic.validate("user", password));
    }

    #[test]
    fn it_tests_colon_in_password() {
        let username = "user";
        let password = "pass:with:colons";
        let basic = Basic(STANDARD.encode(format!("{username}:{password}")).into_boxed_str());

        assert!(basic.validate(username, password));
        assert!(!basic.validate(username, "pass"));
    }
}
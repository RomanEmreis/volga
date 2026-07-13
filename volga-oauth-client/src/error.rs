//! Client-side error model
//!
//! [`ClientError`] separates transport failures from protocol-level OAuth
//! errors: an OAuth error response body (RFC 6749 §5.2) surfaces as
//! [`ClientError::Protocol`] with the parsed [`OAuthError`], everything
//! below it (connection, TLS, timeout, malformed body) as the other
//! variants.

use http::StatusCode;
use std::fmt::{Display, Formatter};
use volga_oauth_core::OAuthError;

/// Error returned by OAuth client operations
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The server returned an OAuth 2.0 error response (RFC 6749 §5.2)
    Protocol(OAuthError),

    /// The server returned an unexpected HTTP status without a parseable
    /// OAuth error body
    Http(StatusCode),

    /// The request could not be completed (connection, TLS or timeout failure)
    Transport(Box<dyn std::error::Error + Send + Sync>),

    /// The response body could not be deserialized
    Decode(serde_json::Error),

    /// A plain `http://` URL was rejected because HTTPS is enforced
    /// (see [`ClientConfig::require_https`](crate::ClientConfig::require_https))
    InsecureUrl(String),

    /// The response failed semantic validation required by the spec
    /// (e.g. the `issuer` in a discovered document does not match the
    /// requested issuer, RFC 8414 §3.3)
    Validation(String),
}

impl ClientError {
    /// Creates a [`ClientError::Transport`] from any error source
    pub fn transport(err: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Transport(err.into())
    }

    /// Creates a [`ClientError::Validation`] with the given reason
    pub fn validation(reason: impl Into<String>) -> Self {
        Self::Validation(reason.into())
    }
}

impl Display for ClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(err) => Display::fmt(err, f),
            Self::Http(status) => write!(f, "unexpected HTTP status: {status}"),
            Self::Transport(err) => write!(f, "transport error: {err}"),
            Self::Decode(err) => write!(f, "malformed response body: {err}"),
            Self::InsecureUrl(url) => write!(f, "insecure URL rejected (HTTPS is enforced): {url}"),
            Self::Validation(reason) => write!(f, "response validation failed: {reason}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Protocol(err) => Some(err),
            Self::Transport(err) => Some(err.as_ref()),
            Self::Decode(err) => Some(err),
            _ => None,
        }
    }
}

impl From<OAuthError> for ClientError {
    #[inline]
    fn from(err: OAuthError) -> Self {
        Self::Protocol(err)
    }
}

impl From<serde_json::Error> for ClientError {
    #[inline]
    fn from(err: serde_json::Error) -> Self {
        Self::Decode(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use volga_oauth_core::OAuthErrorCode;

    #[test]
    fn it_displays_all_variants() {
        let cases: [(ClientError, &str); 6] = [
            (
                OAuthError::new(OAuthErrorCode::InvalidGrant)
                    .with_description("expired")
                    .into(),
                "invalid_grant: expired",
            ),
            (
                ClientError::Http(StatusCode::BAD_GATEWAY),
                "unexpected HTTP status: 502 Bad Gateway",
            ),
            (
                ClientError::transport(std::io::Error::other("connection reset")),
                "transport error: connection reset",
            ),
            (
                serde_json::from_str::<serde_json::Value>("{")
                    .unwrap_err()
                    .into(),
                "malformed response body: EOF while parsing an object at line 1 column 1",
            ),
            (
                ClientError::InsecureUrl("http://auth.example.com".into()),
                "insecure URL rejected (HTTPS is enforced): http://auth.example.com",
            ),
            (
                ClientError::validation("issuer mismatch"),
                "response validation failed: issuer mismatch",
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.to_string(), expected);
        }
    }

    #[test]
    fn it_exposes_error_sources() {
        let err: ClientError = OAuthError::new(OAuthErrorCode::InvalidGrant).into();
        assert!(std::error::Error::source(&err).is_some());

        let err = ClientError::transport(std::io::Error::other("reset"));
        assert!(std::error::Error::source(&err).is_some());

        let err = ClientError::Http(StatusCode::BAD_GATEWAY);
        assert!(std::error::Error::source(&err).is_none());
    }
}

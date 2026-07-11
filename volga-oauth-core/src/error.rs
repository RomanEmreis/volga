//! OAuth 2.0/2.1 error models
//!
//! See [RFC 6749 §5.2](https://www.rfc-editor.org/rfc/rfc6749#section-5.2),
//! [RFC 6750 §3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1) and
//! [RFC 8707 §2](https://www.rfc-editor.org/rfc/rfc8707#section-2).

use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// Machine-readable OAuth 2.0 error code
///
/// Covers the registered codes from RFC 6749 (authorization and token
/// endpoints), RFC 6750 (bearer token usage) and RFC 8707 (resource
/// indicators). Unregistered extension codes are preserved as
/// [`OAuthErrorCode::Other`].
///
/// Serializes to/from its `snake_case` wire form:
/// ```
/// use volga_oauth_core::OAuthErrorCode;
///
/// let code = OAuthErrorCode::InvalidToken;
/// assert_eq!(code.as_str(), "invalid_token");
/// assert_eq!(OAuthErrorCode::from("invalid_token"), code);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
#[non_exhaustive]
pub enum OAuthErrorCode {
    /// The request is missing a required parameter, includes an unsupported
    /// parameter value, repeats a parameter or is otherwise malformed
    InvalidRequest,
    /// Client authentication failed
    InvalidClient,
    /// The provided authorization grant or refresh token is invalid, expired or revoked
    InvalidGrant,
    /// The authenticated client is not authorized to use this authorization grant type
    UnauthorizedClient,
    /// The authorization grant type is not supported by the authorization server
    UnsupportedGrantType,
    /// The requested scope is invalid, unknown or malformed
    InvalidScope,
    /// The resource owner or authorization server denied the request
    AccessDenied,
    /// The authorization server does not support obtaining an authorization code using this method
    UnsupportedResponseType,
    /// The server encountered an unexpected condition
    ServerError,
    /// The server is currently unable to handle the request
    TemporarilyUnavailable,
    /// The access token is expired, revoked, malformed or otherwise invalid (RFC 6750)
    InvalidToken,
    /// The request requires higher privileges than provided by the access token (RFC 6750)
    InsufficientScope,
    /// The requested resource is invalid, missing, unknown or malformed (RFC 8707)
    InvalidTarget,
    /// An unregistered extension error code
    Other(String),
}

impl OAuthErrorCode {
    /// Returns the `snake_case` wire form of this error code
    pub fn as_str(&self) -> &str {
        match self {
            OAuthErrorCode::InvalidRequest => "invalid_request",
            OAuthErrorCode::InvalidClient => "invalid_client",
            OAuthErrorCode::InvalidGrant => "invalid_grant",
            OAuthErrorCode::UnauthorizedClient => "unauthorized_client",
            OAuthErrorCode::UnsupportedGrantType => "unsupported_grant_type",
            OAuthErrorCode::InvalidScope => "invalid_scope",
            OAuthErrorCode::AccessDenied => "access_denied",
            OAuthErrorCode::UnsupportedResponseType => "unsupported_response_type",
            OAuthErrorCode::ServerError => "server_error",
            OAuthErrorCode::TemporarilyUnavailable => "temporarily_unavailable",
            OAuthErrorCode::InvalidToken => "invalid_token",
            OAuthErrorCode::InsufficientScope => "insufficient_scope",
            OAuthErrorCode::InvalidTarget => "invalid_target",
            OAuthErrorCode::Other(code) => code,
        }
    }

    /// Returns the HTTP status code conventionally paired with this error code
    ///
    /// Bearer-usage codes follow RFC 6750 §3.1 (`invalid_token` → 401,
    /// `insufficient_scope` → 403); `invalid_client` maps to 401 and the
    /// remaining token/authorization endpoint codes to 400 per RFC 6749 §5.2,
    /// except `server_error` (500) and `temporarily_unavailable` (503).
    /// Extension codes default to 400.
    pub fn status(&self) -> StatusCode {
        match self {
            OAuthErrorCode::InvalidToken | OAuthErrorCode::InvalidClient => {
                StatusCode::UNAUTHORIZED
            }
            OAuthErrorCode::InsufficientScope | OAuthErrorCode::AccessDenied => {
                StatusCode::FORBIDDEN
            }
            OAuthErrorCode::ServerError => StatusCode::INTERNAL_SERVER_ERROR,
            OAuthErrorCode::TemporarilyUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::BAD_REQUEST,
        }
    }

    /// Maps a wire-form code to a known variant, if any
    fn from_known(code: &str) -> Option<Self> {
        let known = match code {
            "invalid_request" => OAuthErrorCode::InvalidRequest,
            "invalid_client" => OAuthErrorCode::InvalidClient,
            "invalid_grant" => OAuthErrorCode::InvalidGrant,
            "unauthorized_client" => OAuthErrorCode::UnauthorizedClient,
            "unsupported_grant_type" => OAuthErrorCode::UnsupportedGrantType,
            "invalid_scope" => OAuthErrorCode::InvalidScope,
            "access_denied" => OAuthErrorCode::AccessDenied,
            "unsupported_response_type" => OAuthErrorCode::UnsupportedResponseType,
            "server_error" => OAuthErrorCode::ServerError,
            "temporarily_unavailable" => OAuthErrorCode::TemporarilyUnavailable,
            "invalid_token" => OAuthErrorCode::InvalidToken,
            "insufficient_scope" => OAuthErrorCode::InsufficientScope,
            "invalid_target" => OAuthErrorCode::InvalidTarget,
            _ => return None,
        };
        Some(known)
    }
}

impl From<&str> for OAuthErrorCode {
    #[inline]
    fn from(code: &str) -> Self {
        Self::from_known(code).unwrap_or_else(|| OAuthErrorCode::Other(code.into()))
    }
}

impl From<String> for OAuthErrorCode {
    #[inline]
    fn from(code: String) -> Self {
        Self::from_known(&code).unwrap_or(OAuthErrorCode::Other(code))
    }
}

impl From<OAuthErrorCode> for String {
    #[inline]
    fn from(code: OAuthErrorCode) -> Self {
        match code {
            OAuthErrorCode::Other(code) => code,
            known => known.as_str().into(),
        }
    }
}

impl Display for OAuthErrorCode {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// OAuth 2.0 error response per RFC 6749 §5.2
///
/// Serializes to the standard JSON error body returned by token and other
/// OAuth endpoints:
///
/// ```json
/// { "error": "invalid_grant", "error_description": "..." }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthError {
    /// Machine-readable error code
    pub error: OAuthErrorCode,

    /// Human-readable ASCII text providing additional information
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,

    /// URI of a web page with information about the error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_uri: Option<String>,
}

impl OAuthError {
    /// Creates a new error with the given code and no description
    pub fn new(error: OAuthErrorCode) -> Self {
        Self {
            error,
            error_description: None,
            error_uri: None,
        }
    }

    /// Sets the human-readable `error_description`
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.error_description = Some(description.into());
        self
    }

    /// Sets the `error_uri` pointing to a web page with details about the error
    pub fn with_error_uri(mut self, uri: impl Into<String>) -> Self {
        self.error_uri = Some(uri.into());
        self
    }
}

impl From<OAuthErrorCode> for OAuthError {
    #[inline]
    fn from(error: OAuthErrorCode) -> Self {
        Self::new(error)
    }
}

impl Display for OAuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.error_description {
            Some(desc) => write!(f, "{}: {desc}", self.error),
            None => Display::fmt(&self.error, f),
        }
    }
}

impl std::error::Error for OAuthError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_maps_known_codes_to_wire_form() {
        let cases = [
            (OAuthErrorCode::InvalidRequest, "invalid_request"),
            (OAuthErrorCode::InvalidClient, "invalid_client"),
            (OAuthErrorCode::InvalidGrant, "invalid_grant"),
            (OAuthErrorCode::UnauthorizedClient, "unauthorized_client"),
            (
                OAuthErrorCode::UnsupportedGrantType,
                "unsupported_grant_type",
            ),
            (OAuthErrorCode::InvalidScope, "invalid_scope"),
            (OAuthErrorCode::AccessDenied, "access_denied"),
            (
                OAuthErrorCode::UnsupportedResponseType,
                "unsupported_response_type",
            ),
            (OAuthErrorCode::ServerError, "server_error"),
            (
                OAuthErrorCode::TemporarilyUnavailable,
                "temporarily_unavailable",
            ),
            (OAuthErrorCode::InvalidToken, "invalid_token"),
            (OAuthErrorCode::InsufficientScope, "insufficient_scope"),
            (OAuthErrorCode::InvalidTarget, "invalid_target"),
        ];
        for (code, wire) in cases {
            assert_eq!(code.as_str(), wire);
            assert_eq!(OAuthErrorCode::from(wire), code);
            assert_eq!(OAuthErrorCode::from(wire.to_string()), code);
        }
    }

    #[test]
    fn it_preserves_unknown_codes() {
        let code = OAuthErrorCode::from("use_dpop_nonce");
        assert_eq!(code, OAuthErrorCode::Other("use_dpop_nonce".into()));
        assert_eq!(code.as_str(), "use_dpop_nonce");
        assert_eq!(String::from(code), "use_dpop_nonce");
    }

    #[test]
    fn it_serializes_code_as_string() {
        let json = serde_json::to_string(&OAuthErrorCode::InvalidToken).unwrap();
        assert_eq!(json, r#""invalid_token""#);
    }

    #[test]
    fn it_deserializes_code_from_string() {
        let code: OAuthErrorCode = serde_json::from_str(r#""insufficient_scope""#).unwrap();
        assert_eq!(code, OAuthErrorCode::InsufficientScope);

        let code: OAuthErrorCode = serde_json::from_str(r#""something_custom""#).unwrap();
        assert_eq!(code, OAuthErrorCode::Other("something_custom".into()));
    }

    #[test]
    fn it_displays_code() {
        assert_eq!(
            OAuthErrorCode::TemporarilyUnavailable.to_string(),
            "temporarily_unavailable"
        );
    }

    #[test]
    fn it_serializes_error_without_optional_fields() {
        let err = OAuthError::new(OAuthErrorCode::InvalidGrant);
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, r#"{"error":"invalid_grant"}"#);
    }

    #[test]
    fn it_serializes_error_with_all_fields() {
        let err = OAuthError::new(OAuthErrorCode::InvalidRequest)
            .with_description("Missing code_verifier")
            .with_error_uri("https://example.com/errors/invalid_request");
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(
            json,
            r#"{"error":"invalid_request","error_description":"Missing code_verifier","error_uri":"https://example.com/errors/invalid_request"}"#
        );
    }

    #[test]
    fn it_deserializes_error_response() {
        let err: OAuthError = serde_json::from_str(
            r#"{"error":"invalid_token","error_description":"Token has expired"}"#,
        )
        .unwrap();
        assert_eq!(err.error, OAuthErrorCode::InvalidToken);
        assert_eq!(err.error_description.as_deref(), Some("Token has expired"));
        assert!(err.error_uri.is_none());
    }

    #[test]
    fn it_displays_error_with_and_without_description() {
        let err = OAuthError::new(OAuthErrorCode::InvalidToken);
        assert_eq!(err.to_string(), "invalid_token");

        let err = err.with_description("Token has expired");
        assert_eq!(err.to_string(), "invalid_token: Token has expired");
    }

    #[test]
    fn it_converts_code_into_error() {
        let err: OAuthError = OAuthErrorCode::AccessDenied.into();
        assert_eq!(err.error, OAuthErrorCode::AccessDenied);
        assert!(err.error_description.is_none());
    }

    #[test]
    fn it_maps_codes_to_status() {
        let cases = [
            (OAuthErrorCode::InvalidToken, StatusCode::UNAUTHORIZED),
            (OAuthErrorCode::InvalidClient, StatusCode::UNAUTHORIZED),
            (OAuthErrorCode::InsufficientScope, StatusCode::FORBIDDEN),
            (OAuthErrorCode::AccessDenied, StatusCode::FORBIDDEN),
            (
                OAuthErrorCode::ServerError,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                OAuthErrorCode::TemporarilyUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (OAuthErrorCode::InvalidRequest, StatusCode::BAD_REQUEST),
            (OAuthErrorCode::InvalidGrant, StatusCode::BAD_REQUEST),
            (OAuthErrorCode::InvalidTarget, StatusCode::BAD_REQUEST),
            (
                OAuthErrorCode::Other("custom".into()),
                StatusCode::BAD_REQUEST,
            ),
        ];
        for (code, status) in cases {
            assert_eq!(code.status(), status, "code: {code}");
        }
    }
}

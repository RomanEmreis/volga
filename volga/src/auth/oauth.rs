//! OAuth 2.1 / OpenID Connect support
//!
//! Foundation types for building OAuth 2.1 resource servers and clients:
//! * Error models per [RFC 6749 §5.2](https://www.rfc-editor.org/rfc/rfc6749#section-5.2)
//!   and [RFC 6750 §3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1)
//! * Authorization Server Metadata per [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)
//! * Protected Resource Metadata per [RFC 9728](https://www.rfc-editor.org/rfc/rfc9728)
//! * Utilities: the `WWW-Authenticate` Bearer challenge builder and parser,
//!   resource URI canonicalization per [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707)
//!   and well-known metadata URL derivation
//! * Built-in handlers serving the metadata documents from a volga
//!   application: configure with
//!   [`App::with_oauth_resource_metadata`](crate::App::with_oauth_resource_metadata) /
//!   [`App::with_oauth_server_metadata`](crate::App::with_oauth_server_metadata)
//!   (or the `set_*` counterparts, or the `[oauth.resource]`/`[oauth.server]`
//!   config file sections), then serve via
//!   [`App::use_oauth_resource_metadata`](crate::App::use_oauth_resource_metadata),
//!   [`App::use_oauth_server_metadata`](crate::App::use_oauth_server_metadata) and
//!   [`App::use_oidc_metadata`](crate::App::use_oidc_metadata)
//!
//! The protocol-level types are shared with the OAuth client crates through
//! [`volga-oauth-core`](volga_oauth_core) and re-exported here; this module
//! adds the server-side integration on top.

pub use volga_oauth_core::{
    AuthorizationServerMetadata, BearerChallenge, OAuthError, OAuthErrorCode,
    ProtectedResourceMetadata, WELL_KNOWN_AUTHORIZATION_SERVER, WELL_KNOWN_OPENID_CONFIGURATION,
    WELL_KNOWN_PROTECTED_RESOURCE, authorization_server_metadata_url, canonicalize_resource_uri,
    openid_configuration_url, protected_resource_metadata_url,
};

mod handlers;

impl From<OAuthError> for crate::error::Error {
    /// Converts an [`OAuthError`] into a [`volga::Error`](crate::error::Error),
    /// so OAuth failures can be propagated from handlers with `?`. The HTTP
    /// status is derived from the error code via [`OAuthErrorCode::status`].
    #[inline]
    fn from(err: OAuthError) -> Self {
        Self::from_parts(err.error.status(), None, err)
    }
}

#[cfg(test)]
mod tests {
    use super::{OAuthError, OAuthErrorCode};
    use crate::{error::Error, http::StatusCode};

    #[test]
    fn it_converts_oauth_error_into_volga_error() {
        let err: Error = OAuthError::new(OAuthErrorCode::InvalidToken)
            .with_description("Token has expired")
            .into();
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
        assert!(err.instance().is_none());
        assert_eq!(err.to_string(), "invalid_token: Token has expired");
    }
}

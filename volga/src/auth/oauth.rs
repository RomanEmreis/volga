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
//!   application ([`App::use_oauth_resource_metadata`](crate::App::use_oauth_resource_metadata),
//!   [`App::use_oauth_server_metadata`](crate::App::use_oauth_server_metadata),
//!   [`App::use_oidc_metadata`](crate::App::use_oidc_metadata))
//!
//! This module intentionally contains no client flows yet — the discovery
//! client is built on top of these types separately.

pub use error::{OAuthError, OAuthErrorCode};
pub use metadata::{
    AuthorizationServerMetadata, ProtectedResourceMetadata, WELL_KNOWN_AUTHORIZATION_SERVER,
    WELL_KNOWN_OPENID_CONFIGURATION, WELL_KNOWN_PROTECTED_RESOURCE,
};
pub use utils::{
    BearerChallenge, authorization_server_metadata_url, canonicalize_resource_uri,
    openid_configuration_url, protected_resource_metadata_url,
};

mod error;
mod handlers;
mod metadata;
mod utils;

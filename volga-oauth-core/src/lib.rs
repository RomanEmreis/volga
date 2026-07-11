//! Shared OAuth 2.1 / OpenID Connect foundation types for Volga
//!
//! Protocol-level types used by both the `volga` server (metadata serving,
//! bearer challenges) and the OAuth client crates:
//!
//! * Error models per [RFC 6749 §5.2](https://www.rfc-editor.org/rfc/rfc6749#section-5.2)
//!   and [RFC 6750 §3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1)
//! * Authorization Server Metadata per [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)
//! * Protected Resource Metadata per [RFC 9728](https://www.rfc-editor.org/rfc/rfc9728)
//! * The `WWW-Authenticate` Bearer challenge builder and parser
//! * Resource URI canonicalization per [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707)
//!   and well-known metadata URL derivation
//!
//! This crate contains no HTTP I/O. Most applications should depend on
//! `volga` (with the `oauth` feature) or `volga-oauth-client` instead of
//! this crate directly; both re-export these types.

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
mod metadata;
mod utils;

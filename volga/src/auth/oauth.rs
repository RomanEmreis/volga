//! OAuth 2.1 / OpenID Connect support
//!
//! Foundation types for building OAuth 2.1 resource servers and clients:
//! * Error models per [RFC 6749 §5.2](https://www.rfc-editor.org/rfc/rfc6749#section-5.2)
//!   and [RFC 6750 §3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1)
//! * Authorization Server Metadata per [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)
//! * Protected Resource Metadata per [RFC 9728](https://www.rfc-editor.org/rfc/rfc9728)
//! * Utilities: the `WWW-Authenticate` Bearer challenge builder and
//!   resource URI canonicalization per [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707)
//!
//! This module intentionally contains no client or server flows yet — those
//! are built on top of these types (discovery handlers in `volga`, the OAuth
//! client in a separate crate).

pub use error::{OAuthError, OAuthErrorCode};
pub use metadata::{
    AuthorizationServerMetadata, ProtectedResourceMetadata, WELL_KNOWN_AUTHORIZATION_SERVER,
    WELL_KNOWN_OPENID_CONFIGURATION, WELL_KNOWN_PROTECTED_RESOURCE,
};
pub use utils::{BearerChallenge, canonicalize_resource_uri};

mod error;
mod metadata;
mod utils;

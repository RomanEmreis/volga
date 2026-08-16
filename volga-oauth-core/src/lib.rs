//! Shared OAuth 2.1 / OpenID Connect foundation types for Volga
//!
//! Protocol-level types used by both the `volga` server (metadata serving,
//! bearer challenges) and the OAuth client crates:
//!
//! * Error models per [RFC 6749 Section 5.2](https://www.rfc-editor.org/rfc/rfc6749#section-5.2)
//!   and [RFC 6750 Section 3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1)
//! * Authorization Server Metadata per [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)
//! * Protected Resource Metadata per [RFC 9728](https://www.rfc-editor.org/rfc/rfc9728)
//! * Dynamic Client Registration models per [RFC 7591](https://www.rfc-editor.org/rfc/rfc7591)
//! * The `WWW-Authenticate` Bearer challenge builder and parser
//! * Resource URI canonicalization per [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707)
//!   and well-known metadata URL derivation
//! * The registered protocol identifiers both sides agree on ([`grant`],
//!   [`client_auth`], [`token_type`]), the JWS algorithm names
//!   ([`JwsAlgorithm`]), public signing keys ([`PublicJwk`], [`JwkSet`])
//!   and PEM header inspection ([`pem`])
//!
//! This crate contains no HTTP I/O and no cryptography. Most applications
//! should depend on `volga` (with the `oauth` feature) or
//! `volga-oauth-client` instead of this crate directly; both re-export
//! these types.

pub use algorithm::JwsAlgorithm;
pub use error::{OAuthError, OAuthErrorCode};
pub use jwk::{JwkSet, PublicJwk};
pub use metadata::{
    AuthorizationServerMetadata, ProtectedResourceMetadata, WELL_KNOWN_AUTHORIZATION_SERVER,
    WELL_KNOWN_OPENID_CONFIGURATION, WELL_KNOWN_PROTECTED_RESOURCE,
};
pub use registration::{ClientMetadata, ClientRegistrationResponse};
pub use utils::{
    BearerChallenge, authorization_server_metadata_url, canonicalize_resource_uri,
    openid_configuration_url, protected_resource_metadata_url,
};

mod algorithm;
mod error;
pub mod jwk;
mod metadata;
pub mod pem;
pub mod protocol;
mod registration;
mod utils;

#[doc(inline)]
pub use protocol::{auth_scheme, client_auth, grant, token_type};

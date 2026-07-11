//! OAuth 2.1 / OpenID Connect client for Volga
//!
//! Built on the shared protocol types from [`volga-oauth-core`](volga_oauth_core)
//! (re-exported here) and independent of the `volga` server crate — usable
//! from any Tokio application.
//!
//! This crate currently provides the client configuration ([`ClientConfig`])
//! and error model ([`ClientError`]); the discovery client (RFC 8414 /
//! RFC 9728), the Authorization Code + PKCE flow and Dynamic Client
//! Registration (RFC 7591) land incrementally on top of them.

pub use config::{ClientConfig, DEFAULT_MAX_REDIRECTS, DEFAULT_TIMEOUT};
pub use error::ClientError;

// Shared protocol types (`volga::auth::oauth` re-exports the same set)
pub use volga_oauth_core::{
    AuthorizationServerMetadata, BearerChallenge, OAuthError, OAuthErrorCode,
    ProtectedResourceMetadata, WELL_KNOWN_AUTHORIZATION_SERVER, WELL_KNOWN_OPENID_CONFIGURATION,
    WELL_KNOWN_PROTECTED_RESOURCE, authorization_server_metadata_url, canonicalize_resource_uri,
    openid_configuration_url, protected_resource_metadata_url,
};

mod config;
mod error;

//! OAuth 2.1 / OpenID Connect client for Volga
//!
//! Built on the shared protocol types from [`volga-oauth-core`](volga_oauth_core)
//! (re-exported here) and independent of the `volga` server crate — usable
//! from any Tokio application.
//!
//! Currently provides the discovery client ([`DiscoveryClient`]) fetching
//! Authorization Server Metadata (RFC 8414), Protected Resource Metadata
//! (RFC 9728) and OpenID Connect provider configuration, on top of the
//! client configuration ([`ClientConfig`]) and error model
//! ([`ClientError`]). The Authorization Code + PKCE flow and Dynamic
//! Client Registration (RFC 7591) land incrementally on the same
//! foundation.

#[cfg(not(any(feature = "http1", feature = "http2")))]
compile_error!(
    "volga-oauth-client requires at least one of the `http1` or `http2` features to be enabled"
);

pub use cache::MetadataCache;
pub use config::{ClientConfig, DEFAULT_MAX_REDIRECTS, DEFAULT_TIMEOUT};
pub use discovery::DiscoveryClient;
pub use error::ClientError;

// Shared protocol types (`volga::auth::oauth` re-exports the same set)
pub use volga_oauth_core::{
    AuthorizationServerMetadata, BearerChallenge, OAuthError, OAuthErrorCode,
    ProtectedResourceMetadata, WELL_KNOWN_AUTHORIZATION_SERVER, WELL_KNOWN_OPENID_CONFIGURATION,
    WELL_KNOWN_PROTECTED_RESOURCE, authorization_server_metadata_url, canonicalize_resource_uri,
    openid_configuration_url, protected_resource_metadata_url,
};

mod cache;
mod config;
mod discovery;
mod error;
mod transport;

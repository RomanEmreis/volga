//! OAuth 2.1 / OpenID Connect client for Volga
//!
//! Built on the shared protocol types from [`volga-oauth-core`](volga_oauth_core)
//! (re-exported here) and independent of the `volga` server crate — usable
//! from any Tokio application.
//!
//! Provides:
//! * [`DiscoveryClient`] — Authorization Server Metadata (RFC 8414),
//!   Protected Resource Metadata (RFC 9728) and OpenID Connect provider
//!   configuration, with the validation the specs require.
//! * [`OAuthClient`] — the OAuth 2.1 Authorization Code flow with
//!   mandatory PKCE ([`Pkce`], S256 only), refresh tokens and resource
//!   indicators (RFC 8707), plus token persistence through the
//!   [`TokenStore`] abstraction.
//!
//! Both share the transport policy of [`ClientConfig`] and the error
//! model of [`ClientError`]. Dynamic Client Registration (RFC 7591) lands
//! on the same foundation.

#[cfg(not(any(feature = "http1", feature = "http2")))]
compile_error!(
    "volga-oauth-client requires at least one of the `http1` or `http2` features to be enabled"
);

pub use cache::MetadataCache;
pub use client::{
    AuthorizationRequest, AuthorizationRequestBuilder, ClientAuthMethod, OAuthClient,
};
pub use config::{ClientConfig, DEFAULT_MAX_REDIRECTS, DEFAULT_TIMEOUT};
pub use discovery::DiscoveryClient;
pub use error::ClientError;
pub use pkce::{PKCE_METHOD, Pkce};
pub use store::{InMemoryTokenStore, TokenStore};
pub use token::{TokenResponse, TokenSet};

// Shared protocol types (`volga::auth::oauth` re-exports the same set)
pub use volga_oauth_core::{
    AuthorizationServerMetadata, BearerChallenge, OAuthError, OAuthErrorCode,
    ProtectedResourceMetadata, WELL_KNOWN_AUTHORIZATION_SERVER, WELL_KNOWN_OPENID_CONFIGURATION,
    WELL_KNOWN_PROTECTED_RESOURCE, authorization_server_metadata_url, canonicalize_resource_uri,
    openid_configuration_url, protected_resource_metadata_url,
};

mod cache;
mod client;
mod config;
mod discovery;
mod error;
mod pkce;
mod store;
mod token;
mod transport;

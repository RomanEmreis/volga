//! OAuth 2.1 / OpenID Connect client for Volga
//!
//! Built on the shared protocol types from [`volga-oauth-core`](volga_oauth_core)
//! (re-exported here) and independent of the `volga` server crate - usable
//! from any Tokio application.
//!
//! Provides:
//! * [`DiscoveryClient`] - Authorization Server Metadata (RFC 8414),
//!   Protected Resource Metadata (RFC 9728) and OpenID Connect provider
//!   configuration, with the validation the specs require.
//! * [`OAuthClient`] - the OAuth 2.1 Authorization Code flow with
//!   mandatory PKCE ([`Pkce`], S256 only), refresh tokens and resource
//!   indicators (RFC 8707), plus token persistence through the
//!   [`TokenStore`] abstraction.
//! * The grants that authenticate the client itself:
//!   [`client_credentials`](OAuthClient::client_credentials) (RFC 6749
//!   Section 4.4), [`jwt_bearer`](OAuthClient::jwt_bearer) (RFC 7523
//!   Section 2.1) and [`exchange_token`](OAuthClient::exchange_token)
//!   (RFC 8693).
//! * [`RegistrationClient`] - Dynamic Client Registration (RFC 7591).
//! * [`Dpop`] - sender-constrained tokens (RFC 9449): a proof of possession
//!   on every token request, the nonce round a server may demand, and the
//!   proofs a caller attaches to its own resource requests.
//!
//! All of them share the transport policy of [`ClientConfig`] and the
//! error model of [`ClientError`].
//!
//! # Feature flags
//!
//! `http1` (default) and `http2` select the HTTP version; at least one is
//! required. `private-key-jwt` adds `private_key_jwt` client authentication
//! (RFC 7523 Section 2.2) - a client assertion signed with the client's own
//! key - and `dpop` adds sender-constrained tokens (RFC 9449). Both are off
//! by default because they are the only parts of this crate that need a JWS
//! signing backend; the secret-based methods and every grant work without
//! them.

#[cfg(not(any(feature = "http1", feature = "http2")))]
compile_error!(
    "volga-oauth-client requires at least one of the `http1` or `http2` features to be enabled"
);

#[cfg(feature = "private-key-jwt")]
pub use assertion::{DEFAULT_ASSERTION_LIFETIME, PrivateKeyJwt};
pub use cache::MetadataCache;
pub use client::{
    AuthorizationRequest, AuthorizationRequestBuilder, ClientAuthMethod, OAuthClient,
};
pub use config::{ClientConfig, DEFAULT_MAX_REDIRECTS, DEFAULT_TIMEOUT};
pub use discovery::DiscoveryClient;
#[cfg(feature = "dpop")]
pub use dpop::{DPOP_HEADER, DPOP_NONCE_HEADER, Dpop, DpopProof};
pub use error::ClientError;
pub use grants::{
    ClientCredentialsRequest, ExchangedToken, JwtBearerRequest, TokenExchangeRequest,
    TokenExchangeResponse,
};
pub use pkce::{PKCE_METHOD, Pkce};
pub use registration::RegistrationClient;
pub use store::{InMemoryTokenStore, TokenStore};
pub use token::{TokenResponse, TokenSet};

// Shared protocol types (`volga::auth::oauth` re-exports the same set)
pub use volga_oauth_core::{
    AuthorizationServerMetadata, BearerChallenge, ClientMetadata, ClientRegistrationResponse,
    JwkSet, JwsAlgorithm, OAuthError, OAuthErrorCode, ProtectedResourceMetadata, PublicJwk,
    WELL_KNOWN_AUTHORIZATION_SERVER, WELL_KNOWN_OPENID_CONFIGURATION,
    WELL_KNOWN_PROTECTED_RESOURCE, auth_scheme, authorization_server_metadata_url,
    canonicalize_resource_uri, client_auth, grant, jwk, openid_configuration_url, pem,
    protected_resource_metadata_url, token_type,
};

#[cfg(feature = "private-key-jwt")]
mod assertion;
mod cache;
mod client;
mod config;
mod discovery;
#[cfg(feature = "dpop")]
pub mod dpop;
mod error;
mod grants;
#[cfg(any(feature = "private-key-jwt", feature = "dpop"))]
mod jws;
mod pkce;
mod registration;
mod store;
mod token;
mod transport;

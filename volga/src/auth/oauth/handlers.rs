//! Built-in handlers serving OAuth metadata documents
//!
//! * [`App::use_oauth_resource_metadata`] — Protected Resource Metadata
//!   per [RFC 9728 §3](https://www.rfc-editor.org/rfc/rfc9728#section-3)
//! * [`App::use_oauth_server_metadata`] — Authorization Server Metadata
//!   per [RFC 8414 §3](https://www.rfc-editor.org/rfc/rfc8414#section-3)
//! * [`App::use_oidc_metadata`] — the same document at the OpenID Connect
//!   Discovery path per [OIDC Discovery 1.0 §4](https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderConfig)

use serde::Serialize;
use std::sync::Arc;

use crate::{
    App,
    headers::{CacheControl, Header},
};

use super::{
    AuthorizationServerMetadata, ProtectedResourceMetadata, authorization_server_metadata_url,
    openid_configuration_url, protected_resource_metadata_url,
};

impl App {
    /// Serves OAuth 2.0 Protected Resource Metadata (RFC 9728 §3)
    ///
    /// Mounts a `GET` route returning the document as `application/json` at
    /// the well-known path derived from the `resource` identifier per
    /// RFC 9728 §3.1: `/.well-known/oauth-protected-resource`, with the
    /// resource's path appended (e.g. `/.well-known/oauth-protected-resource/v1`
    /// for `https://api.example.com/v1`).
    ///
    /// Accepts anything convertible into [`ProtectedResourceMetadata`],
    /// including the resource identifier itself (`&str` / `String`) for the
    /// minimal document. When bearer authentication is configured and no
    /// explicit [`with_resource_metadata_url`] is set, the derived metadata
    /// URL is advertised automatically in `WWW-Authenticate` challenges
    /// (RFC 9728 §5.1).
    ///
    /// Panics when the `resource` identifier is not a valid `http`/`https`
    /// URI or contains a query — this is a startup misconfiguration.
    ///
    /// [`with_resource_metadata_url`]: crate::auth::BearerAuthConfig::with_resource_metadata_url
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::oauth::ProtectedResourceMetadata};
    ///
    /// let mut app = App::new();
    ///
    /// let metadata = ProtectedResourceMetadata::new("https://api.example.com")
    ///     .with_authorization_servers(["https://auth.example.com"])
    ///     .with_scopes(["read", "write"]);
    ///
    /// app.use_oauth_resource_metadata(metadata);
    /// // GET /.well-known/oauth-protected-resource
    /// ```
    pub fn use_oauth_resource_metadata<M>(&mut self, metadata: M) -> &mut Self
    where
        M: Into<ProtectedResourceMetadata>,
    {
        let metadata = metadata.into();
        let metadata_url = protected_resource_metadata_url(&metadata.resource)
            .unwrap_or_else(|err| panic!("OAuth protected resource metadata: {err}"));

        serve_metadata(self, well_known_route(&metadata_url), metadata);

        // The derived URL's only consumer is the WWW-Authenticate challenge
        // built by bearer auth; without `jwt-auth` nothing reads it
        #[cfg(feature = "jwt-auth")]
        {
            self.oauth_resource_metadata_url = Some(metadata_url);
        }
        self
    }

    /// Serves OAuth 2.0 Authorization Server Metadata (RFC 8414 §3)
    ///
    /// Mounts a `GET` route returning the document as `application/json` at
    /// the well-known path derived from the `issuer` identifier per
    /// RFC 8414 §3.1: `/.well-known/oauth-authorization-server`, with the
    /// issuer's path appended (e.g.
    /// `/.well-known/oauth-authorization-server/tenant1` for
    /// `https://auth.example.com/tenant1`).
    ///
    /// Accepts anything convertible into [`AuthorizationServerMetadata`],
    /// including the issuer identifier itself (`&str` / `String`) for the
    /// minimal document.
    ///
    /// Panics when the `issuer` identifier is not a valid `http`/`https`
    /// URI or contains a query — this is a startup misconfiguration.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::oauth::AuthorizationServerMetadata};
    ///
    /// let mut app = App::new();
    ///
    /// let metadata = AuthorizationServerMetadata::new("https://auth.example.com")
    ///     .with_authorization_endpoint("https://auth.example.com/authorize")
    ///     .with_token_endpoint("https://auth.example.com/token");
    ///
    /// app.use_oauth_server_metadata(metadata);
    /// // GET /.well-known/oauth-authorization-server
    /// ```
    pub fn use_oauth_server_metadata<M>(&mut self, metadata: M) -> &mut Self
    where
        M: Into<AuthorizationServerMetadata>,
    {
        let metadata = metadata.into();
        let metadata_url = authorization_server_metadata_url(&metadata.issuer)
            .unwrap_or_else(|err| panic!("OAuth authorization server metadata: {err}"));

        serve_metadata(self, well_known_route(&metadata_url), metadata);
        self
    }

    /// Serves the metadata document at the OpenID Connect Discovery path
    ///
    /// Mounts a `GET` route returning the document as `application/json` at
    /// the path derived from the `issuer` identifier per
    /// [OIDC Discovery 1.0 §4](https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderConfig):
    /// unlike RFC 8414, `/.well-known/openid-configuration` is appended
    /// **after** the issuer's path (e.g.
    /// `/tenant1/.well-known/openid-configuration` for
    /// `https://auth.example.com/tenant1`).
    ///
    /// Accepts anything convertible into [`AuthorizationServerMetadata`],
    /// including the issuer identifier itself (`&str` / `String`).
    /// OIDC-specific fields required by a compliant provider document
    /// (`subject_types_supported`, `id_token_signing_alg_values_supported`,
    /// `userinfo_endpoint`, …) can be supplied through
    /// [`with_additional_field`](AuthorizationServerMetadata::with_additional_field).
    /// Authorization servers commonly publish the same document at both
    /// discovery paths — chain this with [`use_oauth_server_metadata`]:
    ///
    /// [`use_oauth_server_metadata`]: App::use_oauth_server_metadata
    ///
    /// Panics when the `issuer` identifier is not a valid `http`/`https`
    /// URI or contains a query — this is a startup misconfiguration.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::oauth::AuthorizationServerMetadata};
    ///
    /// let mut app = App::new();
    ///
    /// let metadata = AuthorizationServerMetadata::new("https://auth.example.com")
    ///     .with_token_endpoint("https://auth.example.com/token");
    ///
    /// app.use_oauth_server_metadata(metadata.clone())
    ///     .use_oidc_metadata(metadata);
    /// // GET /.well-known/oauth-authorization-server
    /// // GET /.well-known/openid-configuration
    /// ```
    pub fn use_oidc_metadata<M>(&mut self, metadata: M) -> &mut Self
    where
        M: Into<AuthorizationServerMetadata>,
    {
        let metadata = metadata.into();
        let metadata_url = openid_configuration_url(&metadata.issuer)
            .unwrap_or_else(|err| panic!("OpenID Connect discovery metadata: {err}"));

        serve_metadata(self, well_known_route(&metadata_url), metadata);
        self
    }
}

/// Extracts the absolute-path part of a derived metadata URL to mount the
/// route at (`https://host/.well-known/…` → `/.well-known/…`).
fn well_known_route(metadata_url: &str) -> &str {
    let after_scheme = metadata_url.find("://").expect("derived URL is absolute") + 3;
    let path_start = metadata_url[after_scheme..]
        .find('/')
        .expect("derived URL contains the well-known path");
    &metadata_url[after_scheme + path_start..]
}

/// Mounts a `GET` route serving the document as JSON; metadata documents
/// are immutable, so responses carry a public one-hour cache policy.
fn serve_metadata<T>(app: &mut App, path: &str, metadata: T)
where
    T: Serialize + Send + Sync + 'static,
{
    let cache_control = metadata_cache_control();
    let metadata = Arc::new(metadata);
    app.map_get(path, move || {
        let metadata = Arc::clone(&metadata);
        let cache_control = cache_control.clone();
        async move { crate::ok!(metadata.as_ref(); [cache_control]) }
    });
}

fn metadata_cache_control() -> Header<CacheControl> {
    Header::try_from(CacheControl::default().with_public().with_max_age(3600))
        .expect("valid cache control header")
}

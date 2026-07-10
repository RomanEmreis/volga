//! Configuration and built-in handlers serving OAuth metadata documents
//!
//! Metadata is configured on [`App`] with `with_*` (closure) / `set_*`
//! (whole value, including the `&str` identifier shorthand) and served by
//! the parameterless `use_*` methods:
//!
//! * [`App::use_oauth_resource_metadata`] — Protected Resource Metadata
//!   per [RFC 9728 §3](https://www.rfc-editor.org/rfc/rfc9728#section-3)
//! * [`App::use_oauth_server_metadata`] — Authorization Server Metadata
//!   per [RFC 8414 §3](https://www.rfc-editor.org/rfc/rfc8414#section-3)
//! * [`App::use_oidc_metadata`] — the same document at the OpenID Connect
//!   Discovery path per [OIDC Discovery 1.0 §4](https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderConfig)
//!
//! With the `config` feature, both documents can also be provided by the
//! `[oauth.resource]` and `[oauth.server]` sections of the configuration
//! file; the file overrides prior builder calls.

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

const RESOURCE_METADATA_NOT_CONFIGURED: &str = "OAuth protected resource metadata is not configured. \
    Use `App::with_oauth_resource_metadata` or `App::set_oauth_resource_metadata` to configure it.";

const SERVER_METADATA_NOT_CONFIGURED: &str = "OAuth authorization server metadata is not configured. \
    Use `App::with_oauth_server_metadata` or `App::set_oauth_server_metadata` to configure it.";

impl App {
    /// Configures OAuth 2.0 Protected Resource Metadata (RFC 9728) via a
    /// builder closure
    ///
    /// The closure receives the currently configured document (e.g. from the
    /// `[oauth.resource]` config file section or an earlier builder call) or
    /// a default one. Call [`use_oauth_resource_metadata`] to serve it.
    ///
    /// [`use_oauth_resource_metadata`]: App::use_oauth_resource_metadata
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new().with_oauth_resource_metadata(|metadata| metadata
    ///     .with_resource("https://api.example.com")
    ///     .with_authorization_servers(["https://auth.example.com"])
    ///     .with_scopes(["read", "write"]));
    ///
    /// app.use_oauth_resource_metadata();
    /// ```
    pub fn with_oauth_resource_metadata<F>(mut self, config: F) -> Self
    where
        F: FnOnce(ProtectedResourceMetadata) -> ProtectedResourceMetadata,
    {
        let metadata = self.oauth_resource_metadata.take().unwrap_or_default();
        self.oauth_resource_metadata = Some(config(metadata));
        self
    }

    /// Sets the OAuth 2.0 Protected Resource Metadata (RFC 9728)
    ///
    /// Accepts anything convertible into [`ProtectedResourceMetadata`],
    /// including the resource identifier itself (`&str` / `String`) for the
    /// minimal document. Call [`use_oauth_resource_metadata`] to serve it.
    ///
    /// [`use_oauth_resource_metadata`]: App::use_oauth_resource_metadata
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new()
    ///     .set_oauth_resource_metadata("https://api.example.com");
    ///
    /// app.use_oauth_resource_metadata();
    /// ```
    pub fn set_oauth_resource_metadata<M>(mut self, metadata: M) -> Self
    where
        M: Into<ProtectedResourceMetadata>,
    {
        self.oauth_resource_metadata = Some(metadata.into());
        self
    }

    /// Configures OAuth 2.0 Authorization Server Metadata (RFC 8414) via a
    /// builder closure
    ///
    /// The closure receives the currently configured document (e.g. from the
    /// `[oauth.server]` config file section or an earlier builder call) or a
    /// default one carrying the [`AuthorizationServerMetadata::new`]
    /// prefills (`response_types_supported = ["code"]`,
    /// `grant_types_supported = ["authorization_code"]`). Call
    /// [`use_oauth_server_metadata`] and/or [`use_oidc_metadata`] to serve it.
    ///
    /// [`use_oauth_server_metadata`]: App::use_oauth_server_metadata
    /// [`use_oidc_metadata`]: App::use_oidc_metadata
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new().with_oauth_server_metadata(|metadata| metadata
    ///     .with_issuer("https://auth.example.com")
    ///     .with_authorization_endpoint("https://auth.example.com/authorize")
    ///     .with_token_endpoint("https://auth.example.com/token"));
    ///
    /// app.use_oauth_server_metadata();
    /// ```
    pub fn with_oauth_server_metadata<F>(mut self, config: F) -> Self
    where
        F: FnOnce(AuthorizationServerMetadata) -> AuthorizationServerMetadata,
    {
        let metadata = self
            .oauth_server_metadata
            .take()
            // seed with `new()` rather than `Default` to keep the OAuth 2.1
            // response/grant type prefills when the closure builds from scratch
            .unwrap_or_else(|| AuthorizationServerMetadata::new(""));
        self.oauth_server_metadata = Some(config(metadata));
        self
    }

    /// Sets the OAuth 2.0 Authorization Server Metadata (RFC 8414)
    ///
    /// Accepts anything convertible into [`AuthorizationServerMetadata`],
    /// including the issuer identifier itself (`&str` / `String`) for the
    /// minimal document. Call [`use_oauth_server_metadata`] and/or
    /// [`use_oidc_metadata`] to serve it.
    ///
    /// [`use_oauth_server_metadata`]: App::use_oauth_server_metadata
    /// [`use_oidc_metadata`]: App::use_oidc_metadata
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new()
    ///     .set_oauth_server_metadata("https://auth.example.com");
    ///
    /// app.use_oauth_server_metadata();
    /// ```
    pub fn set_oauth_server_metadata<M>(mut self, metadata: M) -> Self
    where
        M: Into<AuthorizationServerMetadata>,
    {
        self.oauth_server_metadata = Some(metadata.into());
        self
    }

    /// Serves the configured OAuth 2.0 Protected Resource Metadata (RFC 9728 §3)
    ///
    /// Mounts a `GET` route returning the document as `application/json` at
    /// the well-known path derived from the `resource` identifier per
    /// RFC 9728 §3.1: `/.well-known/oauth-protected-resource`, with the
    /// resource's path appended (e.g. `/.well-known/oauth-protected-resource/v1`
    /// for `https://api.example.com/v1`).
    ///
    /// When bearer authentication is configured and no explicit
    /// [`with_resource_metadata_url`] is set, the derived metadata URL is
    /// advertised automatically in `WWW-Authenticate` challenges
    /// (RFC 9728 §5.1).
    ///
    /// Panics when no document is configured (via
    /// [`with_oauth_resource_metadata`], [`set_oauth_resource_metadata`] or
    /// the `[oauth.resource]` config file section), or when the `resource`
    /// identifier is not a valid `http`/`https` URI or contains a query —
    /// these are startup misconfigurations.
    ///
    /// [`with_resource_metadata_url`]: crate::auth::BearerAuthConfig::with_resource_metadata_url
    /// [`with_oauth_resource_metadata`]: App::with_oauth_resource_metadata
    /// [`set_oauth_resource_metadata`]: App::set_oauth_resource_metadata
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new().with_oauth_resource_metadata(|metadata| metadata
    ///     .with_resource("https://api.example.com")
    ///     .with_authorization_servers(["https://auth.example.com"])
    ///     .with_scopes(["read", "write"]));
    ///
    /// app.use_oauth_resource_metadata();
    /// // GET /.well-known/oauth-protected-resource
    /// ```
    pub fn use_oauth_resource_metadata(&mut self) -> &mut Self {
        let metadata = self
            .oauth_resource_metadata
            .clone()
            .expect(RESOURCE_METADATA_NOT_CONFIGURED);
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

    /// Serves the configured OAuth 2.0 Authorization Server Metadata (RFC 8414 §3)
    ///
    /// Mounts a `GET` route returning the document as `application/json` at
    /// the well-known path derived from the `issuer` identifier per
    /// RFC 8414 §3.1: `/.well-known/oauth-authorization-server`, with the
    /// issuer's path appended (e.g.
    /// `/.well-known/oauth-authorization-server/tenant1` for
    /// `https://auth.example.com/tenant1`).
    ///
    /// Panics when no document is configured (via
    /// [`with_oauth_server_metadata`], [`set_oauth_server_metadata`] or the
    /// `[oauth.server]` config file section), or when the `issuer`
    /// identifier is not a valid `http`/`https` URI or contains a query —
    /// these are startup misconfigurations.
    ///
    /// [`with_oauth_server_metadata`]: App::with_oauth_server_metadata
    /// [`set_oauth_server_metadata`]: App::set_oauth_server_metadata
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new().with_oauth_server_metadata(|metadata| metadata
    ///     .with_issuer("https://auth.example.com")
    ///     .with_authorization_endpoint("https://auth.example.com/authorize")
    ///     .with_token_endpoint("https://auth.example.com/token"));
    ///
    /// app.use_oauth_server_metadata();
    /// // GET /.well-known/oauth-authorization-server
    /// ```
    pub fn use_oauth_server_metadata(&mut self) -> &mut Self {
        let metadata = self
            .oauth_server_metadata
            .clone()
            .expect(SERVER_METADATA_NOT_CONFIGURED);
        let metadata_url = authorization_server_metadata_url(&metadata.issuer)
            .unwrap_or_else(|err| panic!("OAuth authorization server metadata: {err}"));

        serve_metadata(self, well_known_route(&metadata_url), metadata);
        self
    }

    /// Serves the configured metadata document at the OpenID Connect
    /// Discovery path
    ///
    /// Mounts a `GET` route returning the same document configured for
    /// [`use_oauth_server_metadata`] as `application/json` at the path
    /// derived from the `issuer` identifier per
    /// [OIDC Discovery 1.0 §4](https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderConfig):
    /// unlike RFC 8414, `/.well-known/openid-configuration` is appended
    /// **after** the issuer's path (e.g.
    /// `/tenant1/.well-known/openid-configuration` for
    /// `https://auth.example.com/tenant1`).
    ///
    /// OIDC-specific fields required by a compliant provider document
    /// (`subject_types_supported`, `id_token_signing_alg_values_supported`,
    /// `userinfo_endpoint`, …) can be supplied through
    /// [`with_additional_field`](AuthorizationServerMetadata::with_additional_field).
    /// Authorization servers commonly publish the same document at both
    /// discovery paths — chain this with [`use_oauth_server_metadata`]:
    ///
    /// [`use_oauth_server_metadata`]: App::use_oauth_server_metadata
    ///
    /// Panics when no document is configured (via
    /// [`with_oauth_server_metadata`](App::with_oauth_server_metadata),
    /// [`set_oauth_server_metadata`](App::set_oauth_server_metadata) or the
    /// `[oauth.server]` config file section), or when the `issuer`
    /// identifier is not a valid `http`/`https` URI or contains a query —
    /// these are startup misconfigurations.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new().with_oauth_server_metadata(|metadata| metadata
    ///     .with_issuer("https://auth.example.com")
    ///     .with_token_endpoint("https://auth.example.com/token"));
    ///
    /// app.use_oauth_server_metadata()
    ///     .use_oidc_metadata();
    /// // GET /.well-known/oauth-authorization-server
    /// // GET /.well-known/openid-configuration
    /// ```
    pub fn use_oidc_metadata(&mut self) -> &mut Self {
        let metadata = self
            .oauth_server_metadata
            .clone()
            .expect(SERVER_METADATA_NOT_CONFIGURED);
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

#[cfg(test)]
mod tests {
    use crate::App;

    #[test]
    fn it_composes_oauth_metadata_builder_calls() {
        let app = App::new()
            .set_oauth_resource_metadata("https://api.example.com")
            .with_oauth_resource_metadata(|metadata| metadata.with_scopes(["read"]))
            .set_oauth_server_metadata("https://auth.example.com")
            .with_oauth_server_metadata(|metadata| {
                metadata.with_token_endpoint("https://auth.example.com/token")
            });

        let resource = app.oauth_resource_metadata.as_ref().unwrap();
        assert_eq!(resource.resource, "https://api.example.com");
        assert_eq!(resource.scopes_supported, ["read"]);

        let server = app.oauth_server_metadata.as_ref().unwrap();
        assert_eq!(server.issuer, "https://auth.example.com");
        assert_eq!(
            server.token_endpoint.as_deref(),
            Some("https://auth.example.com/token")
        );
        // the `new()` prefills survive the closure composition
        assert_eq!(server.response_types_supported, ["code"]);
        assert_eq!(server.grant_types_supported, ["authorization_code"]);
    }

    #[test]
    fn it_seeds_server_metadata_closure_with_oauth21_prefills() {
        let app = App::new().with_oauth_server_metadata(|metadata| {
            metadata.with_issuer("https://auth.example.com")
        });

        let server = app.oauth_server_metadata.as_ref().unwrap();
        assert_eq!(server.response_types_supported, ["code"]);
        assert_eq!(server.grant_types_supported, ["authorization_code"]);
    }

    #[test]
    #[should_panic(expected = "OAuth protected resource metadata is not configured")]
    fn it_panics_when_resource_metadata_is_not_configured() {
        App::new().use_oauth_resource_metadata();
    }

    #[test]
    #[should_panic(expected = "OAuth authorization server metadata is not configured")]
    fn it_panics_when_server_metadata_is_not_configured() {
        App::new().use_oauth_server_metadata();
    }

    #[test]
    #[should_panic(expected = "OAuth authorization server metadata is not configured")]
    fn it_panics_when_oidc_metadata_is_not_configured() {
        App::new().use_oidc_metadata();
    }
}

//! OAuth 2.1 client
//!
//! [`OAuthClient`] implements the Authorization Code flow with mandatory
//! PKCE, refresh tokens and resource indicators (RFC 8707) on top of
//! server metadata — typically discovered with
//! [`DiscoveryClient`](crate::DiscoveryClient).

use base64::{Engine, engine::general_purpose::STANDARD};
use http::HeaderValue;
use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use volga_oauth_core::{AuthorizationServerMetadata, OAuthErrorCode};

use crate::{
    ClientConfig, ClientError, Pkce, TokenResponse, TokenSet, TokenStore,
    pkce::{PKCE_METHOD, random_urlsafe},
    transport::Transport,
};

/// How early before its expiration a stored access token is considered
/// stale by [`OAuthClient::token`] and refreshed
const EXPIRY_LEEWAY: Duration = Duration::from_secs(30);

const TOKEN_STORE_NOT_CONFIGURED: &str =
    "OAuth client: token store is not configured; attach one with with_token_store(..)";

/// How a confidential client authenticates to the token endpoint
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClientAuthMethod {
    /// `client_secret_basic` — HTTP Basic authentication (RFC 6749
    /// §2.3.1), the default and the method servers are required to support
    #[default]
    Basic,

    /// `client_secret_post` — credentials in the request body, for
    /// servers that do not accept HTTP Basic authentication
    Post,
}

/// OAuth 2.1 client for the Authorization Code + PKCE flow
///
/// Without a secret the client acts as a public client (PKCE is the
/// protection, as OAuth 2.1 prescribes); with one it authenticates to the
/// token endpoint per the configured [`ClientAuthMethod`].
///
/// # Example
/// ```no_run
/// use std::sync::Arc;
/// use volga_oauth_client::{DiscoveryClient, InMemoryTokenStore, OAuthClient};
///
/// # async fn run() -> Result<(), volga_oauth_client::ClientError> {
/// let metadata = DiscoveryClient::new()
///     .fetch_server_metadata("https://auth.example.com")
///     .await?;
///
/// let client = OAuthClient::new("my-client")
///     .with_redirect_uri("https://app.example.com/callback")
///     .with_token_store(Arc::new(InMemoryTokenStore::new()));
///
/// let auth = client
///     .authorization_request(&metadata)
///     .with_scopes(["read"])
///     .with_resource("https://api.example.com")
///     .build()?;
///
/// // send the user to `auth.url`; then, in the redirect callback:
/// # let (code, state) = ("code", "state");
/// assert!(auth.matches_state(state));
/// let tokens = client.exchange_code(&metadata, code, &auth).await?;
/// client.store_tokens("alice", &tokens);
///
/// // later — served from the store, transparently refreshed when stale:
/// let tokens = client.token("alice", &metadata).await?;
/// # Ok(())
/// # }
/// ```
pub struct OAuthClient {
    transport: Transport,
    client_id: String,
    client_secret: Option<String>,
    auth_method: ClientAuthMethod,
    redirect_uri: Option<String>,
    store: Option<Arc<dyn TokenStore>>,
}

impl OAuthClient {
    /// Creates a public client with the given `client_id` and the default
    /// [`ClientConfig`]
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            transport: Transport::new(ClientConfig::new()),
            client_id: client_id.into(),
            client_secret: None,
            auth_method: ClientAuthMethod::default(),
            redirect_uri: None,
            store: None,
        }
    }

    /// Replaces the transport configuration
    pub fn with_config(mut self, config: ClientConfig) -> Self {
        self.transport = Transport::new(config);
        self
    }

    /// Makes this a confidential client authenticating to the token
    /// endpoint with `client_secret`
    pub fn with_secret(mut self, client_secret: impl Into<String>) -> Self {
        self.client_secret = Some(client_secret.into());
        self
    }

    /// Sets how the client secret is presented to the token endpoint;
    /// [`ClientAuthMethod::Basic`] by default, ignored without a secret
    pub fn with_auth_method(mut self, method: ClientAuthMethod) -> Self {
        self.auth_method = method;
        self
    }

    /// Sets the `redirect_uri` sent in authorization and token requests
    pub fn with_redirect_uri(mut self, redirect_uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(redirect_uri.into());
        self
    }

    /// Attaches a [`TokenStore`] enabling [`token`](Self::token) and
    /// [`store_tokens`](Self::store_tokens)
    pub fn with_token_store(mut self, store: Arc<dyn TokenStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Starts building an authorization request against `metadata`
    ///
    /// [`AuthorizationRequestBuilder::build`] produces the URL to send the
    /// user to, along with the generated `state` and PKCE pair.
    pub fn authorization_request<'a>(
        &'a self,
        metadata: &'a AuthorizationServerMetadata,
    ) -> AuthorizationRequestBuilder<'a> {
        AuthorizationRequestBuilder {
            client: self,
            metadata,
            scopes: Vec::new(),
            resources: Vec::new(),
            state: None,
            extra: Vec::new(),
        }
    }

    /// Exchanges an authorization `code` for tokens (RFC 6749 §4.1.3)
    ///
    /// `request` is the [`AuthorizationRequest`] the code was obtained
    /// with: it supplies the PKCE verifier and repeats the requested
    /// resource indicators. Verify the callback `state` with
    /// [`AuthorizationRequest::matches_state`] before calling this.
    pub async fn exchange_code(
        &self,
        metadata: &AuthorizationServerMetadata,
        code: &str,
        request: &AuthorizationRequest,
    ) -> Result<TokenSet, ClientError> {
        let endpoint = token_endpoint(metadata)?;
        let mut form = form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "authorization_code")
            .append_pair("code", code)
            .append_pair("code_verifier", request.pkce.verifier());
        if let Some(redirect_uri) = &self.redirect_uri {
            form.append_pair("redirect_uri", redirect_uri);
        }
        for resource in &request.resources {
            form.append_pair("resource", resource);
        }
        let authorization = self.apply_client_auth(&mut form);
        self.request_tokens(endpoint, form.finish(), authorization)
            .await
    }

    /// Obtains fresh tokens with a refresh token (RFC 6749 §6)
    ///
    /// The server may rotate the refresh token; when the response carries
    /// none, the one passed in remains valid — [`token`](Self::token)
    /// handles that carry-over automatically.
    pub async fn refresh(
        &self,
        metadata: &AuthorizationServerMetadata,
        refresh_token: &str,
    ) -> Result<TokenSet, ClientError> {
        let endpoint = token_endpoint(metadata)?;
        let mut form = form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "refresh_token")
            .append_pair("refresh_token", refresh_token);
        let authorization = self.apply_client_auth(&mut form);
        self.request_tokens(endpoint, form.finish(), authorization)
            .await
    }

    /// Returns valid tokens stored under `key`, refreshing a stale access
    /// token transparently
    ///
    /// `Ok(None)` means interactive authorization is required: nothing is
    /// stored, the stored entry has no refresh token to renew it with, or
    /// the server rejected the refresh token (`invalid_grant`) — in the
    /// latter cases the dead entry is removed from the store.
    ///
    /// # Panics
    /// Panics when no [`TokenStore`] is attached
    /// (see [`with_token_store`](Self::with_token_store)).
    pub async fn token(
        &self,
        key: &str,
        metadata: &AuthorizationServerMetadata,
    ) -> Result<Option<TokenSet>, ClientError> {
        let store = self.store.as_deref().expect(TOKEN_STORE_NOT_CONFIGURED);
        let Some(tokens) = store.get(key) else {
            return Ok(None);
        };
        if !tokens.expires_within(EXPIRY_LEEWAY) {
            return Ok(Some(tokens));
        }
        let Some(refresh_token) = tokens.refresh_token else {
            store.remove(key);
            return Ok(None);
        };
        match self.refresh(metadata, &refresh_token).await {
            Ok(mut fresh) => {
                // no rotation in the response — the old token stays valid
                if fresh.refresh_token.is_none() {
                    fresh.refresh_token = Some(refresh_token);
                }
                store.put(key, &fresh);
                Ok(Some(fresh))
            }
            Err(ClientError::Protocol(err)) if err.error == OAuthErrorCode::InvalidGrant => {
                store.remove(key);
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    /// Stores `tokens` under `key` — typically right after
    /// [`exchange_code`](Self::exchange_code)
    ///
    /// # Panics
    /// Panics when no [`TokenStore`] is attached
    /// (see [`with_token_store`](Self::with_token_store)).
    pub fn store_tokens(&self, key: &str, tokens: &TokenSet) {
        self.store
            .as_deref()
            .expect(TOKEN_STORE_NOT_CONFIGURED)
            .put(key, tokens);
    }

    async fn request_tokens(
        &self,
        endpoint: &str,
        body: String,
        authorization: Option<HeaderValue>,
    ) -> Result<TokenSet, ClientError> {
        let value = self
            .transport
            .post_form(endpoint, body, authorization)
            .await?;
        let response: TokenResponse = serde_json::from_value(value)?;
        Ok(response.into())
    }

    /// Applies client authentication to a token request: either an HTTP
    /// Basic header or credentials appended to `form`, per the configured
    /// method. Public clients identify themselves with `client_id` alone.
    fn apply_client_auth(
        &self,
        form: &mut form_urlencoded::Serializer<'_, String>,
    ) -> Option<HeaderValue> {
        match (&self.client_secret, self.auth_method) {
            (Some(secret), ClientAuthMethod::Basic) => {
                Some(basic_credentials(&self.client_id, secret))
            }
            (Some(secret), ClientAuthMethod::Post) => {
                form.append_pair("client_id", &self.client_id)
                    .append_pair("client_secret", secret);
                None
            }
            (None, _) => {
                form.append_pair("client_id", &self.client_id);
                None
            }
        }
    }
}

impl std::fmt::Debug for OAuthClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthClient")
            .field("transport", &self.transport)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[redacted]"),
            )
            .field("auth_method", &self.auth_method)
            .field("redirect_uri", &self.redirect_uri)
            .field("store", &self.store.as_ref().map(|_| "dyn TokenStore"))
            .finish()
    }
}

/// Builder for an authorization request, created by
/// [`OAuthClient::authorization_request`]
pub struct AuthorizationRequestBuilder<'a> {
    client: &'a OAuthClient,
    metadata: &'a AuthorizationServerMetadata,
    scopes: Vec<String>,
    resources: Vec<String>,
    state: Option<String>,
    extra: Vec<(String, String)>,
}

impl AuthorizationRequestBuilder<'_> {
    /// Sets the requested scopes, joined into the `scope` parameter
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// Adds a resource indicator (RFC 8707), repeatable; it is also sent
    /// with the token request by [`OAuthClient::exchange_code`]
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resources.push(resource.into());
        self
    }

    /// Overrides the `state` parameter; a random value is generated when
    /// not set
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// Adds an extra query parameter (e.g. the OIDC `nonce` or `prompt`),
    /// repeatable
    pub fn with_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.push((name.into(), value.into()));
        self
    }

    /// Builds the authorization URL together with the `state` and PKCE
    /// pair the application must keep for the callback
    ///
    /// Fails with [`ClientError::Validation`] when the metadata declares
    /// no `authorization_endpoint` or advertises PKCE methods without
    /// `S256` (OAuth 2.1 requires it).
    pub fn build(self) -> Result<AuthorizationRequest, ClientError> {
        let endpoint = self
            .metadata
            .authorization_endpoint
            .as_deref()
            .ok_or_else(|| {
                ClientError::validation("server metadata declares no authorization_endpoint")
            })?;
        self.client.transport.check_scheme(endpoint)?;

        let methods = &self.metadata.code_challenge_methods_supported;
        if !methods.is_empty() && !methods.iter().any(|method| method == PKCE_METHOD) {
            return Err(ClientError::validation(format!(
                "authorization server does not support the {PKCE_METHOD} PKCE method"
            )));
        }

        let pkce = Pkce::new();
        let state = self.state.unwrap_or_else(|| random_urlsafe(16));

        let mut query = form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client.client_id)
            .append_pair("state", &state)
            .append_pair("code_challenge", pkce.challenge())
            .append_pair("code_challenge_method", PKCE_METHOD);
        if let Some(redirect_uri) = &self.client.redirect_uri {
            query.append_pair("redirect_uri", redirect_uri);
        }
        if !self.scopes.is_empty() {
            query.append_pair("scope", &self.scopes.join(" "));
        }
        for resource in &self.resources {
            query.append_pair("resource", resource);
        }
        for (name, value) in &self.extra {
            query.append_pair(name, value);
        }
        let query = query.finish();

        let separator = if endpoint.contains('?') { '&' } else { '?' };
        Ok(AuthorizationRequest {
            url: format!("{endpoint}{separator}{query}"),
            state,
            pkce,
            resources: self.resources,
        })
    }
}

impl std::fmt::Debug for AuthorizationRequestBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationRequestBuilder")
            .field("scopes", &self.scopes)
            .field("resources", &self.resources)
            .field("state", &self.state)
            .field("extra", &self.extra)
            .finish_non_exhaustive()
    }
}

/// A prepared authorization request
///
/// Everything the application must keep between redirecting the user to
/// [`url`](Self::url) and exchanging the callback code; serializable so a
/// web application can stash it in the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    /// The authorization URL to send the user to
    pub url: String,

    /// The `state` parameter embedded in the URL; the callback must echo
    /// it back (checked with [`matches_state`](Self::matches_state))
    pub state: String,

    /// The PKCE pair; the verifier is sent with the token request
    pub pkce: Pkce,

    /// The requested resource indicators, repeated in the token request
    /// per RFC 8707
    pub resources: Vec<String>,
}

impl AuthorizationRequest {
    /// Returns `true` when the `state` returned by the callback matches
    /// the one this request was built with — always verify it before
    /// exchanging the code (CSRF protection)
    #[inline]
    pub fn matches_state(&self, state: &str) -> bool {
        self.state == state
    }
}

/// Builds an RFC 6749 §2.3.1 HTTP Basic authorization header: identifier
/// and secret are form-urlencoded before being joined and base64-encoded.
fn basic_credentials(client_id: &str, client_secret: &str) -> HeaderValue {
    let encode =
        |value: &str| -> String { form_urlencoded::byte_serialize(value.as_bytes()).collect() };
    let credentials = STANDARD.encode(format!("{}:{}", encode(client_id), encode(client_secret)));
    HeaderValue::from_str(&format!("Basic {credentials}"))
        .expect("base64 output is always a valid header value")
}

fn token_endpoint(metadata: &AuthorizationServerMetadata) -> Result<&str, ClientError> {
    metadata
        .token_endpoint
        .as_deref()
        .ok_or_else(|| ClientError::validation("server metadata declares no token_endpoint"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> AuthorizationServerMetadata {
        let mut metadata = AuthorizationServerMetadata::new("https://auth.example.com");
        metadata.authorization_endpoint = Some("https://auth.example.com/authorize".into());
        metadata.token_endpoint = Some("https://auth.example.com/token".into());
        metadata
    }

    fn query_pairs(url: &str) -> Vec<(String, String)> {
        let query = url.split_once('?').unwrap().1;
        form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect()
    }

    #[test]
    fn it_builds_a_spec_compliant_authorization_url() {
        let client =
            OAuthClient::new("my-client").with_redirect_uri("https://app.example.com/callback");
        let request = client
            .authorization_request(&metadata())
            .with_scopes(["read", "write"])
            .with_resource("https://api.example.com")
            .with_param("nonce", "n-1")
            .build()
            .unwrap();

        assert!(
            request
                .url
                .starts_with("https://auth.example.com/authorize?")
        );
        let pairs = query_pairs(&request.url);
        let get = |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(get("response_type"), Some("code"));
        assert_eq!(get("client_id"), Some("my-client"));
        assert_eq!(
            get("redirect_uri"),
            Some("https://app.example.com/callback")
        );
        assert_eq!(get("scope"), Some("read write"));
        assert_eq!(get("resource"), Some("https://api.example.com"));
        assert_eq!(get("code_challenge"), Some(request.pkce.challenge()));
        assert_eq!(get("code_challenge_method"), Some("S256"));
        assert_eq!(get("state"), Some(request.state.as_str()));
        assert_eq!(get("nonce"), Some("n-1"));
        assert!(request.matches_state(&request.state.clone()));
        assert!(!request.matches_state("other"));
    }

    #[test]
    fn it_appends_to_an_existing_query_and_respects_custom_state() {
        let mut metadata = metadata();
        metadata.authorization_endpoint =
            Some("https://auth.example.com/authorize?tenant=t1".into());
        let request = OAuthClient::new("my-client")
            .authorization_request(&metadata)
            .with_state("custom-state")
            .build()
            .unwrap();
        assert!(request.url.contains("tenant=t1&response_type=code"));
        assert_eq!(request.state, "custom-state");
    }

    #[test]
    fn it_validates_metadata_before_building_requests() {
        let client = OAuthClient::new("my-client");

        let mut incomplete = metadata();
        incomplete.authorization_endpoint = None;
        assert!(matches!(
            client.authorization_request(&incomplete).build(),
            Err(ClientError::Validation(reason)) if reason.contains("authorization_endpoint")
        ));

        let mut plain_only = metadata();
        plain_only.code_challenge_methods_supported = vec!["plain".into()];
        assert!(matches!(
            client.authorization_request(&plain_only).build(),
            Err(ClientError::Validation(reason)) if reason.contains("S256")
        ));

        // https enforcement applies to the authorization endpoint too
        let mut insecure = metadata();
        insecure.authorization_endpoint = Some("http://auth.example.com/authorize".into());
        assert!(matches!(
            client.authorization_request(&insecure).build(),
            Err(ClientError::InsecureUrl(_))
        ));
    }

    #[test]
    fn it_encodes_basic_credentials_per_rfc6749() {
        // RFC 6749 §2.3.1: form-urlencode the id and secret first
        let header = basic_credentials("client with space", "s&cret");
        let encoded = header
            .to_str()
            .unwrap()
            .strip_prefix("Basic ")
            .unwrap()
            .to_owned();
        let decoded = String::from_utf8(STANDARD.decode(encoded).unwrap()).unwrap();
        assert_eq!(decoded, "client+with+space:s%26cret");
    }

    #[test]
    fn it_applies_the_configured_client_authentication() {
        let public = OAuthClient::new("my-client");
        let mut form = form_urlencoded::Serializer::new(String::new());
        assert!(public.apply_client_auth(&mut form).is_none());
        assert_eq!(form.finish(), "client_id=my-client");

        let basic = OAuthClient::new("my-client").with_secret("s3cret");
        let mut form = form_urlencoded::Serializer::new(String::new());
        assert!(basic.apply_client_auth(&mut form).is_some());
        assert_eq!(form.finish(), "");

        let post = OAuthClient::new("my-client")
            .with_secret("s3cret")
            .with_auth_method(ClientAuthMethod::Post);
        let mut form = form_urlencoded::Serializer::new(String::new());
        assert!(post.apply_client_auth(&mut form).is_none());
        assert_eq!(form.finish(), "client_id=my-client&client_secret=s3cret");
    }

    #[test]
    #[should_panic(expected = "token store is not configured")]
    fn it_panics_on_store_access_without_a_store() {
        OAuthClient::new("my-client").store_tokens(
            "alice",
            &TokenSet {
                access_token: "at".into(),
                token_type: "Bearer".into(),
                refresh_token: None,
                scope: None,
                id_token: None,
                expires_at: None,
            },
        );
    }
}

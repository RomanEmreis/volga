//! OAuth 2.1 client
//!
//! [`OAuthClient`] implements the Authorization Code flow with mandatory
//! PKCE, refresh tokens and resource indicators (RFC 8707) on top of
//! server metadata - typically discovered with
//! [`DiscoveryClient`](crate::DiscoveryClient).

use base64::{Engine, engine::general_purpose::STANDARD};
use http::HeaderValue;
use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use volga_oauth_core::{AuthorizationServerMetadata, OAuthErrorCode};

#[cfg(feature = "private-key-jwt")]
use crate::PrivateKeyJwt;
use crate::{
    ClientConfig, ClientError, Pkce, TokenResponse, TokenSet, TokenStore, client_auth, grant,
    pkce::{PKCE_METHOD, random_urlsafe},
    transport::Transport,
};

/// How early before its expiration a stored access token is considered
/// stale by [`OAuthClient::token`] and refreshed
pub(crate) const EXPIRY_LEEWAY: Duration = Duration::from_secs(30);

const TOKEN_STORE_NOT_CONFIGURED: &str =
    "OAuth client: token store is not configured; attach one with with_token_store(..)";

/// How a confidential client authenticates to the token endpoint
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClientAuthMethod {
    /// `client_secret_basic` - HTTP Basic authentication (RFC 6749
    /// Section 2.3.1), the default and the method servers are required to support
    #[default]
    Basic,

    /// `client_secret_post` - credentials in the request body, for
    /// servers that do not accept HTTP Basic authentication
    Post,

    /// `private_key_jwt` - a client assertion signed with the client's own
    /// key (RFC 7523 Section 2.2), so no shared secret ever leaves it
    ///
    /// Set through
    /// [`OAuthClient::with_private_key_jwt`](OAuthClient::with_private_key_jwt);
    /// unlike the two secret-based methods it needs no
    /// [`with_secret`](OAuthClient::with_secret).
    ///
    /// Requires the `private-key-jwt` feature, the only part of this crate
    /// that needs a JWS signing backend.
    #[cfg(feature = "private-key-jwt")]
    PrivateKeyJwt(PrivateKeyJwt),
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
/// // later - served from the store, transparently refreshed when stale:
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
    /// The `grant_types` of the registration this client was built from,
    /// as returned - `None` when it never went through one, which is the
    /// only case that constrains nothing. An empty list is *not* that: per
    /// RFC 7591 Section 2 it means `authorization_code` alone, which
    /// [`grant::covers`] applies.
    registered_grant_types: Option<Vec<String>>,
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
            registered_grant_types: None,
        }
    }

    /// Creates a client from a Dynamic Client Registration response
    /// (RFC 7591), adopting the issued credentials
    ///
    /// The registered `token_endpoint_auth_method` selects the
    /// [`ClientAuthMethod`] (`client_secret_basic` when omitted, per
    /// RFC 7591 Section 2) and, when exactly one `redirect_uri` was registered,
    /// it becomes the client's redirect URI.
    ///
    /// Fails with [`ClientError::Validation`] when the registered method
    /// is one this client cannot perform (e.g. `client_secret_jwt`) -
    /// authenticating differently from the registration would only yield
    /// `invalid_client` at the token endpoint. A registration with `none`
    /// produces a public client; a secret issued alongside it is ignored,
    /// since that method sends no credentials.
    ///
    /// A registration for `private_key_jwt` is rejected here because this
    /// constructor is given no key to sign assertions with - pass one to
    /// `from_registration_with_key` instead, which the `private-key-jwt`
    /// feature enables.
    pub fn from_registration(
        response: &volga_oauth_core::ClientRegistrationResponse,
    ) -> Result<Self, ClientError> {
        let mut client = Self::new(response.client_id.clone());

        // an omitted method defaults to client_secret_basic (RFC 7591 Section 2)
        match response
            .metadata
            .token_endpoint_auth_method
            .as_deref()
            .unwrap_or(client_auth::CLIENT_SECRET_BASIC)
        {
            client_auth::NONE => {}
            client_auth::CLIENT_SECRET_BASIC => {
                if let Some(secret) = &response.client_secret {
                    client = client.with_secret(secret.clone());
                }
            }
            client_auth::CLIENT_SECRET_POST => {
                if let Some(secret) = &response.client_secret {
                    client = client
                        .with_secret(secret.clone())
                        .with_auth_method(ClientAuthMethod::Post);
                }
            }
            // performable only with a key to sign the assertions with
            #[cfg(feature = "private-key-jwt")]
            client_auth::PRIVATE_KEY_JWT => {
                return Err(ClientError::validation(
                    "registered token_endpoint_auth_method 'private_key_jwt' needs a signing \
                     key; use OAuthClient::from_registration_with_key",
                ));
            }
            unsupported => {
                return Err(ClientError::validation(format!(
                    "registered token_endpoint_auth_method '{unsupported}' is not supported; \
                     this client supports {}, {} and {}{}",
                    client_auth::CLIENT_SECRET_BASIC,
                    client_auth::CLIENT_SECRET_POST,
                    client_auth::NONE,
                    if cfg!(feature = "private-key-jwt") {
                        ", plus private_key_jwt through from_registration_with_key"
                    } else {
                        " (private_key_jwt needs the `private-key-jwt` feature)"
                    },
                )));
            }
        }

        Ok(client.adopt_registered_metadata(response))
    }

    /// Creates a client from a Dynamic Client Registration response that
    /// registered `private_key_jwt`, signing its assertions with `key`
    ///
    /// Behaves exactly like [`from_registration`](Self::from_registration)
    /// for every other registered method - `key` is then simply unused,
    /// since sending an assertion the registration did not announce would
    /// only yield `invalid_client`.
    ///
    /// Fails with [`ClientError::Validation`] when the registration pins
    /// `token_endpoint_auth_signing_alg` and `key` signs with something
    /// else. That field is per-client and narrower than the server-wide
    /// list an assertion is checked against, so a key disagreeing with it
    /// would satisfy discovery and still be refused at the token endpoint.
    #[cfg(feature = "private-key-jwt")]
    pub fn from_registration_with_key(
        response: &volga_oauth_core::ClientRegistrationResponse,
        key: PrivateKeyJwt,
    ) -> Result<Self, ClientError> {
        if response.metadata.token_endpoint_auth_method.as_deref()
            != Some(client_auth::PRIVATE_KEY_JWT)
        {
            return Self::from_registration(response);
        }

        if let Some(registered) = response.metadata.token_endpoint_auth_signing_alg.as_deref()
            && registered != key.algorithm().as_str()
        {
            return Err(ClientError::validation(format!(
                "the registration signs client assertions with {registered}, but the key \
                 supplied signs with {}",
                key.algorithm()
            )));
        }

        // when the registration inlines the client's JWK Set, the server
        // looks the assertion's `kid` up in it - a key naming one that is
        // not there is refused at the token endpoint, whatever it signs
        if let Some(key_id) = key.key_id()
            && let Some(registered) = registered_key_ids(response)
            && !registered.iter().any(|candidate| candidate == key_id)
        {
            return Err(ClientError::validation(format!(
                "the key is identified by '{key_id}', which the registered JWK Set does not \
                 hold; it published {registered:?}"
            )));
        }

        Ok(Self::new(response.client_id.clone())
            .with_private_key_jwt(key)
            .adopt_registered_metadata(response))
    }

    /// Adopts what the registration approved: the redirect URI when exactly
    /// one was registered, and the grant types when it named any.
    fn adopt_registered_metadata(
        mut self,
        response: &volga_oauth_core::ClientRegistrationResponse,
    ) -> Self {
        if let [redirect_uri] = response.metadata.redirect_uris.as_slice() {
            self = self.with_redirect_uri(redirect_uri.clone());
        }

        self.registered_grant_types = Some(response.metadata.grant_types.clone());
        self
    }

    /// Refuses a grant this client's registration did not approve.
    ///
    /// The token endpoint answers `unauthorized_client` for one it did not,
    /// and that is a harder failure to read than this. Only a client that
    /// never went through a registration is unconstrained - an omitted
    /// `grant_types` is `authorization_code` alone, not carte blanche
    /// (RFC 7591 Section 2, applied by [`grant::covers`]).
    ///
    /// `refresh_token` is exempt. RFC 6749 Section 6 makes it the
    /// continuation of a grant the client already holds rather than a
    /// separate authorization, and servers routinely issue refresh tokens
    /// without naming the grant in a registration - refusing it here would
    /// break those clients for a check the token endpoint never applies.
    pub(crate) fn ensure_grant_registered(&self, grant_type: &str) -> Result<(), ClientError> {
        let Some(registered) = &self.registered_grant_types else {
            return Ok(());
        };

        if grant_type == grant::REFRESH_TOKEN || grant::covers(registered, grant_type) {
            return Ok(());
        }

        Err(ClientError::validation(format!(
            "this client is not registered for the '{grant_type}' grant; its registration \
             approved {registered:?}"
        )))
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

    /// Sets how the client authenticates to the token endpoint;
    /// [`ClientAuthMethod::Basic`] by default
    ///
    /// The two secret-based methods are ignored without a secret - the
    /// client then stays public. `ClientAuthMethod::PrivateKeyJwt` carries
    /// its own credential and applies on its own; prefer
    /// `with_private_key_jwt` for it (feature `private-key-jwt`).
    pub fn with_auth_method(mut self, method: ClientAuthMethod) -> Self {
        self.auth_method = method;
        self
    }

    /// Makes this a confidential client authenticating with a
    /// `private_key_jwt` client assertion (RFC 7523 Section 2.2)
    ///
    /// Supersedes any [`with_secret`](Self::with_secret): the assertion is
    /// the credential, and no secret is sent alongside it.
    #[cfg(feature = "private-key-jwt")]
    pub fn with_private_key_jwt(mut self, key: PrivateKeyJwt) -> Self {
        self.auth_method = ClientAuthMethod::PrivateKeyJwt(key);
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

    /// Exchanges an authorization `code` for tokens (RFC 6749 Section 4.1.3)
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
        self.ensure_grant_registered(grant::AUTHORIZATION_CODE)?;

        let endpoint = token_endpoint(metadata)?;
        // the serializer is not `Sync`: scoping it keeps it out of the
        // future's state, so this future stays `Send` (see `refresh`)
        let (body, authorization) = {
            let mut form = form_urlencoded::Serializer::new(String::new());

            form.append_pair("grant_type", grant::AUTHORIZATION_CODE)
                .append_pair("code", code)
                .append_pair("code_verifier", request.pkce.verifier());

            if let Some(redirect_uri) = &self.redirect_uri {
                form.append_pair("redirect_uri", redirect_uri);
            }

            for resource in &request.resources {
                form.append_pair("resource", resource);
            }

            let authorization = self.apply_client_auth(&mut form, metadata)?;
            (form.finish(), authorization)
        };

        self.request_tokens(endpoint, body, authorization).await
    }

    /// Obtains fresh tokens with a refresh token (RFC 6749 Section 6)
    ///
    /// The server may rotate the refresh token; when the response carries
    /// none, the one passed in remains valid - [`token`](Self::token)
    /// handles that carry-over automatically.
    pub async fn refresh(
        &self,
        metadata: &AuthorizationServerMetadata,
        refresh_token: &str,
    ) -> Result<TokenSet, ClientError> {
        let endpoint = token_endpoint(metadata)?;
        // scoped so the non-`Sync` serializer is dropped before the await:
        // a future holding it would be `!Send` and could not be spawned
        let (body, authorization) = {
            let mut form = form_urlencoded::Serializer::new(String::new());

            form.append_pair("grant_type", grant::REFRESH_TOKEN)
                .append_pair("refresh_token", refresh_token);

            let authorization = self.apply_client_auth(&mut form, metadata)?;
            (form.finish(), authorization)
        };

        self.request_tokens(endpoint, body, authorization).await
    }

    /// Returns valid tokens stored under `key`, refreshing a stale access
    /// token transparently
    ///
    /// `Ok(None)` means interactive authorization is required: nothing is
    /// stored, the stored entry has no refresh token to renew it with, or
    /// the server rejected the refresh token (`invalid_grant`) - in the
    /// latter cases the dead entry is removed from the store.
    ///
    /// This is the Authorization Code counterpart, where renewal means the
    /// refresh token. A client acting for itself has none to renew with;
    /// see [`ClientCredentialsRequest::token`](crate::ClientCredentialsRequest::token),
    /// which re-runs the grant instead.
    ///
    /// # Panics
    /// Panics when no [`TokenStore`] is attached
    /// (see [`with_token_store`](Self::with_token_store)).
    pub async fn token(
        &self,
        key: &str,
        metadata: &AuthorizationServerMetadata,
    ) -> Result<Option<TokenSet>, ClientError> {
        let store = self.token_store();
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
                // no rotation in the response - the old token stays valid
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

    /// Stores `tokens` under `key` - typically right after
    /// [`exchange_code`](Self::exchange_code)
    ///
    /// # Panics
    /// Panics when no [`TokenStore`] is attached
    /// (see [`with_token_store`](Self::with_token_store)).
    pub fn store_tokens(&self, key: &str, tokens: &TokenSet) {
        self.token_store().put(key, tokens);
    }

    /// The attached [`TokenStore`].
    ///
    /// # Panics
    /// Panics when none is attached - the same contract every public
    /// store-backed method carries.
    pub(crate) fn token_store(&self) -> &dyn TokenStore {
        self.store.as_deref().expect(TOKEN_STORE_NOT_CONFIGURED)
    }

    pub(crate) async fn request_tokens(
        &self,
        endpoint: &str,
        body: String,
        authorization: Option<HeaderValue>,
    ) -> Result<TokenSet, ClientError> {
        let response: TokenResponse = self
            .post_token_request(endpoint, body, authorization)
            .await?;

        Ok(response.into())
    }

    /// Submits a prepared token request and deserializes the successful
    /// response into `T`; every grant goes through here.
    pub(crate) async fn post_token_request<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: String,
        authorization: Option<HeaderValue>,
    ) -> Result<T, ClientError> {
        let mut headers = http::HeaderMap::new();
        if let Some(authorization) = authorization {
            headers.insert(http::header::AUTHORIZATION, authorization);
        }

        let value = self
            .transport
            .post_form(endpoint, body, headers)
            .await?
            .into_json()?;

        serde_json::from_value(value).map_err(Into::into)
    }

    /// Applies client authentication to a token request: either an HTTP
    /// Basic header or credentials appended to `form`, per the configured
    /// method. Public clients identify themselves with `client_id` alone.
    ///
    /// Every credential-bearing method is checked against
    /// `token_endpoint_auth_methods_supported` first: sending one the
    /// server never announced only earns an `invalid_client` over the
    /// network. A public client is not checked - it presents no credential
    /// for the server to have a method for.
    pub(crate) fn apply_client_auth(
        &self,
        form: &mut form_urlencoded::Serializer<'_, String>,
        metadata: &AuthorizationServerMetadata,
    ) -> Result<Option<HeaderValue>, ClientError> {
        // the assertion is the credential; a secret, if any, is not sent
        #[cfg(feature = "private-key-jwt")]
        if let ClientAuthMethod::PrivateKeyJwt(key) = &self.auth_method {
            ensure_auth_method_supported(metadata, client_auth::PRIVATE_KEY_JWT)?;

            form.append_pair("client_id", &self.client_id)
                .append_pair(
                    "client_assertion_type",
                    client_auth::ASSERTION_TYPE_JWT_BEARER,
                )
                .append_pair(
                    "client_assertion",
                    &key.assertion(&self.client_id, metadata)?,
                );

            return Ok(None);
        }

        Ok(match (&self.client_secret, &self.auth_method) {
            (Some(secret), ClientAuthMethod::Basic) => {
                ensure_auth_method_supported(metadata, client_auth::CLIENT_SECRET_BASIC)?;
                Some(basic_credentials(&self.client_id, secret))
            }
            (Some(secret), ClientAuthMethod::Post) => {
                ensure_auth_method_supported(metadata, client_auth::CLIENT_SECRET_POST)?;
                form.append_pair("client_id", &self.client_id)
                    .append_pair("client_secret", secret);
                None
            }
            _ => {
                form.append_pair("client_id", &self.client_id);
                None
            }
        })
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
            .field("registered_grant_types", &self.registered_grant_types)
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
        // caught before the URL exists, rather than after a user has been
        // redirected into a flow this client may not complete
        self.client
            .ensure_grant_registered(grant::AUTHORIZATION_CODE)?;

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
    /// the one this request was built with - always verify it before
    /// exchanging the code (CSRF protection)
    #[inline]
    pub fn matches_state(&self, state: &str) -> bool {
        self.state == state
    }

    /// Validates the parameters of the authorization callback before the
    /// code is exchanged: the `state` (CSRF) and the RFC 9207 `iss`
    /// (authorization server mix-up).
    ///
    /// `iss` is the callback's `iss` query parameter, `None` when the
    /// response carried none. It must match the issuer whenever it is
    /// present, and it is *required* when the metadata advertises
    /// [`authorization_response_iss_parameter_supported`] - a response
    /// missing it there may come from a different, possibly malicious,
    /// authorization server.
    ///
    /// ```no_run
    /// # use volga_oauth_client::{
    /// #     AuthorizationRequest, AuthorizationServerMetadata, ClientError,
    /// # };
    /// # fn check(
    /// #     request: &AuthorizationRequest,
    /// #     metadata: &AuthorizationServerMetadata,
    /// #     state: &str,
    /// #     iss: Option<&str>,
    /// # ) -> Result<(), ClientError> {
    /// request.validate_callback(metadata, state, iss)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`authorization_response_iss_parameter_supported`]: AuthorizationServerMetadata::authorization_response_iss_parameter_supported
    pub fn validate_callback(
        &self,
        metadata: &AuthorizationServerMetadata,
        state: &str,
        iss: Option<&str>,
    ) -> Result<(), ClientError> {
        if !self.matches_state(state) {
            return Err(ClientError::validation(
                "authorization response `state` does not match the request",
            ));
        }

        match (iss, metadata.authorization_response_iss_parameter_supported) {
            (Some(iss), _) if iss != metadata.issuer => Err(ClientError::validation(format!(
                "authorization response `iss` mismatch: expected {}, got {iss}",
                metadata.issuer
            ))),
            (None, true) => Err(ClientError::validation(
                "authorization server advertises RFC 9207 but the response carries no `iss`",
            )),
            _ => Ok(()),
        }
    }
}

/// Builds an RFC 6749 Section 2.3.1 HTTP Basic authorization header: identifier
/// and secret are form-urlencoded before being joined and base64-encoded.
fn basic_credentials(client_id: &str, client_secret: &str) -> HeaderValue {
    let encode =
        |value: &str| -> String { form_urlencoded::byte_serialize(value.as_bytes()).collect() };

    let credentials = STANDARD.encode(format!("{}:{}", encode(client_id), encode(client_secret)));

    HeaderValue::from_str(&format!("Basic {credentials}"))
        .expect("base64 output is always a valid header value")
}

/// The `kid`s of the JWK Set a registration response inlines, or `None`
/// when it inlines none.
///
/// Read leniently out of the raw document rather than through
/// [`JwkSet`](volga_oauth_core::JwkSet): a registration may echo a set this
/// framework would refuse to model - an encryption key, a curve it does not
/// sign with - and that is no reason to fail. Only the identifiers matter
/// here, and a set that names none constrains nothing.
#[cfg(feature = "private-key-jwt")]
fn registered_key_ids(
    response: &volga_oauth_core::ClientRegistrationResponse,
) -> Option<Vec<String>> {
    let key_ids: Vec<String> = response
        .metadata
        .jwks
        .as_ref()?
        .get("keys")?
        .as_array()?
        .iter()
        .filter_map(|key| key.get("kid")?.as_str().map(ToOwned::to_owned))
        .collect();

    (!key_ids.is_empty()).then_some(key_ids)
}

/// Refuses a client authentication method the server does not announce in
/// `token_endpoint_auth_methods_supported`.
///
/// A metadata document that lists none is not second-guessed - that is what
/// a hand-built [`AuthorizationServerMetadata`] carries. A *discovered* one
/// always lists something: RFC 8414 Section 2 defines the default for an
/// omitted field as `client_secret_basic`, which deserialization
/// materializes, so a server silent about the field is taken at the spec's
/// word rather than assumed to accept anything.
fn ensure_auth_method_supported(
    metadata: &AuthorizationServerMetadata,
    method: &str,
) -> Result<(), ClientError> {
    let advertised = &metadata.token_endpoint_auth_methods_supported;
    if advertised.is_empty() || advertised.iter().any(|candidate| candidate == method) {
        return Ok(());
    }

    Err(ClientError::validation(format!(
        "authorization server does not accept {method} at the token endpoint; \
         it advertises {advertised:?}"
    )))
}

pub(crate) fn token_endpoint(metadata: &AuthorizationServerMetadata) -> Result<&str, ClientError> {
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
        // RFC 6749 Section 2.3.1: form-urlencode the id and secret first
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
        let metadata = metadata();
        let apply = |client: &OAuthClient| {
            let mut form = form_urlencoded::Serializer::new(String::new());
            let authorization = client.apply_client_auth(&mut form, &metadata).unwrap();
            (authorization, form.finish())
        };

        let (authorization, body) = apply(&OAuthClient::new("my-client"));
        assert!(authorization.is_none());
        assert_eq!(body, "client_id=my-client");

        let (authorization, body) = apply(&OAuthClient::new("my-client").with_secret("s3cret"));
        assert!(authorization.is_some());
        assert_eq!(body, "");

        let (authorization, body) = apply(
            &OAuthClient::new("my-client")
                .with_secret("s3cret")
                .with_auth_method(ClientAuthMethod::Post),
        );
        assert!(authorization.is_none());
        assert_eq!(body, "client_id=my-client&client_secret=s3cret");
    }

    #[cfg(feature = "private-key-jwt")]
    #[test]
    fn it_authenticates_with_a_signed_client_assertion() {
        // a secret set alongside the key is never sent: the assertion is
        // the credential
        let client = OAuthClient::new("my-client")
            .with_secret("s3cret")
            .with_private_key_jwt(crate::assertion::test_key());

        let metadata = metadata();
        let mut form = form_urlencoded::Serializer::new(String::new());
        let authorization = client.apply_client_auth(&mut form, &metadata).unwrap();
        assert!(authorization.is_none());

        let body = form.finish();
        let pairs: Vec<_> = form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
        let get = |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(get("client_id").as_deref(), Some("my-client"));
        assert_eq!(
            get("client_assertion_type").as_deref(),
            Some(client_auth::ASSERTION_TYPE_JWT_BEARER)
        );
        assert_eq!(get("client_secret"), None);
        assert_eq!(
            get("client_assertion").unwrap().split('.').count(),
            3,
            "the assertion must be a compact JWS"
        );
    }

    #[test]
    fn it_refuses_an_unadvertised_client_authentication_method() {
        let apply = |client: &OAuthClient, metadata: &AuthorizationServerMetadata| {
            let mut form = form_urlencoded::Serializer::new(String::new());
            client.apply_client_auth(&mut form, metadata).map(|_| ())
        };
        let advertising = |methods: &[&str]| {
            let mut metadata = metadata();
            metadata.token_endpoint_auth_methods_supported =
                methods.iter().map(|method| (*method).to_owned()).collect();
            metadata
        };

        let basic = OAuthClient::new("my-client").with_secret("s3cret");
        let post = OAuthClient::new("my-client")
            .with_secret("s3cret")
            .with_auth_method(ClientAuthMethod::Post);
        #[cfg(feature = "private-key-jwt")]
        let assertion =
            OAuthClient::new("my-client").with_private_key_jwt(crate::assertion::test_key());

        #[allow(unused_mut)]
        let mut clients = vec![
            (&basic, client_auth::CLIENT_SECRET_BASIC),
            (&post, client_auth::CLIENT_SECRET_POST),
        ];
        #[cfg(feature = "private-key-jwt")]
        clients.push((&assertion, client_auth::PRIVATE_KEY_JWT));

        for (client, method) in clients {
            // a server announcing only other methods refuses this one
            let others = advertising(&["client_secret_jwt", "tls_client_auth"]);
            let err = apply(client, &others).unwrap_err();
            assert!(
                matches!(&err, ClientError::Validation(reason) if reason.contains(method)),
                "{method}: {err}"
            );

            // ...it goes through once announced
            assert!(apply(client, &advertising(&[method])).is_ok(), "{method}");

            // ...and metadata announcing nothing is not second-guessed,
            // which is what a hand-built document carries
            assert!(apply(client, &metadata()).is_ok(), "{method}");
        }

        // a discovered document silent about the field materializes the
        // RFC 8414 default, so it accepts Basic and nothing else
        let discovered: AuthorizationServerMetadata = serde_json::from_value(serde_json::json!({
            "issuer": "https://auth.example.com",
            "response_types_supported": ["code"],
        }))
        .unwrap();
        assert!(apply(&basic, &discovered).is_ok());
        assert!(apply(&post, &discovered).is_err());

        // a public client presents no credential, so there is no method for
        // the server to have announced
        assert!(apply(&OAuthClient::new("my-client"), &discovered).is_ok());
    }

    #[cfg(feature = "private-key-jwt")]
    #[test]
    fn it_reports_a_failing_client_assertion() {
        let mut metadata = metadata();
        metadata.token_endpoint_auth_signing_alg_values_supported = vec!["RS256".into()];

        let client =
            OAuthClient::new("my-client").with_private_key_jwt(crate::assertion::test_key());
        let mut form = form_urlencoded::Serializer::new(String::new());
        assert!(matches!(
            client.apply_client_auth(&mut form, &metadata),
            Err(ClientError::Validation(reason)) if reason.contains("ES256")
        ));
    }

    #[test]
    fn it_adopts_registered_credentials_per_auth_method() {
        let registration = |auth_method: serde_json::Value| {
            serde_json::from_value::<volga_oauth_core::ClientRegistrationResponse>(
                serde_json::json!({
                    "client_id": "generated-id",
                    "client_secret": "generated-secret",
                    "token_endpoint_auth_method": auth_method,
                    "redirect_uris": ["https://app.example.com/callback"]
                }),
            )
            .unwrap()
        };

        // an omitted method defaults to client_secret_basic
        let client =
            OAuthClient::from_registration(&registration(serde_json::Value::Null)).unwrap();
        assert_eq!(client.auth_method, ClientAuthMethod::Basic);
        assert_eq!(client.client_secret.as_deref(), Some("generated-secret"));
        assert_eq!(
            client.redirect_uri.as_deref(),
            Some("https://app.example.com/callback")
        );

        let client =
            OAuthClient::from_registration(&registration("client_secret_post".into())).unwrap();
        assert_eq!(client.auth_method, ClientAuthMethod::Post);
        assert_eq!(client.client_secret.as_deref(), Some("generated-secret"));

        // `none` sends no credentials - the client stays public even
        // though the server issued a secret
        let client = OAuthClient::from_registration(&registration("none".into())).unwrap();
        assert_eq!(client.client_secret, None);

        // a method this client cannot perform is rejected upfront rather
        // than failing with invalid_client at the token endpoint
        let err =
            OAuthClient::from_registration(&registration("client_secret_jwt".into())).unwrap_err();
        assert!(matches!(
            err,
            ClientError::Validation(reason) if reason.contains("client_secret_jwt")
        ));

        // private_key_jwt is announced as needing a key when this client can
        // sign at all, and as needing the feature when it cannot
        let err =
            OAuthClient::from_registration(&registration("private_key_jwt".into())).unwrap_err();
        let expected = if cfg!(feature = "private-key-jwt") {
            "from_registration_with_key"
        } else {
            "private-key-jwt"
        };
        assert!(
            matches!(&err, ClientError::Validation(reason) if reason.contains(expected)),
            "got: {err}"
        );
    }

    #[cfg(feature = "private-key-jwt")]
    #[test]
    fn it_adopts_a_registration_signing_with_a_key() {
        let registration = |auth_method: &str| {
            serde_json::from_value::<volga_oauth_core::ClientRegistrationResponse>(
                serde_json::json!({
                    "client_id": "generated-id",
                    "client_secret": "generated-secret",
                    "token_endpoint_auth_method": auth_method,
                    "redirect_uris": ["https://app.example.com/callback"]
                }),
            )
            .unwrap()
        };

        let key = crate::assertion::test_key();
        let client =
            OAuthClient::from_registration_with_key(&registration("private_key_jwt"), key.clone())
                .unwrap();
        assert_eq!(client.auth_method, ClientAuthMethod::PrivateKeyJwt(key));
        assert_eq!(client.client_secret, None);
        assert_eq!(
            client.redirect_uri.as_deref(),
            Some("https://app.example.com/callback")
        );

        // ...and a key handed to a registration that announced something
        // else stays unused
        let client = OAuthClient::from_registration_with_key(
            &registration("client_secret_post"),
            crate::assertion::test_key(),
        )
        .unwrap();
        assert_eq!(client.auth_method, ClientAuthMethod::Post);
        assert_eq!(client.client_secret.as_deref(), Some("generated-secret"));
    }

    #[cfg(feature = "private-key-jwt")]
    #[test]
    fn it_honors_the_registered_client_assertion_algorithm() {
        let registration = |signing_alg: serde_json::Value| {
            serde_json::from_value::<volga_oauth_core::ClientRegistrationResponse>(
                serde_json::json!({
                    "client_id": "generated-id",
                    "token_endpoint_auth_method": "private_key_jwt",
                    "token_endpoint_auth_signing_alg": signing_alg,
                }),
            )
            .unwrap()
        };

        // the test key signs ES256; a registration pinning RS256 would have
        // the token endpoint answer `invalid_client`, however capable the
        // authorization server itself is
        let err = OAuthClient::from_registration_with_key(
            &registration("RS256".into()),
            crate::assertion::test_key(),
        )
        .unwrap_err();
        assert!(
            matches!(&err, ClientError::Validation(reason)
                if reason.contains("RS256") && reason.contains("ES256")),
            "got: {err}"
        );

        // the matching algorithm, and an unpinned registration, go through
        for signing_alg in [serde_json::json!("ES256"), serde_json::Value::Null] {
            assert!(
                OAuthClient::from_registration_with_key(
                    &registration(signing_alg.clone()),
                    crate::assertion::test_key(),
                )
                .is_ok(),
                "refused {signing_alg}"
            );
        }
    }

    #[cfg(feature = "private-key-jwt")]
    #[test]
    fn it_matches_the_key_against_the_registered_jwk_set() {
        let registration = |jwks: serde_json::Value| {
            serde_json::from_value::<volga_oauth_core::ClientRegistrationResponse>(
                serde_json::json!({
                    "client_id": "generated-id",
                    "token_endpoint_auth_method": "private_key_jwt",
                    "jwks": jwks,
                }),
            )
            .unwrap()
        };
        let published = |kid: &str| serde_json::json!({ "keys": [{ "kty": "EC", "kid": kid }] });
        let with_key_id = || crate::assertion::test_key().with_key_id("2026-08");

        // the server resolves the assertion's `kid` in the registered set;
        // one that is not there is refused at the token endpoint
        let err = OAuthClient::from_registration_with_key(
            &registration(published("other")),
            with_key_id(),
        )
        .unwrap_err();
        assert!(
            matches!(&err, ClientError::Validation(reason)
                if reason.contains("2026-08") && reason.contains("other")),
            "got: {err}"
        );

        // a matching `kid` goes through...
        assert!(
            OAuthClient::from_registration_with_key(
                &registration(published("2026-08")),
                with_key_id()
            )
            .is_ok()
        );

        // ...and so does anything that constrains nothing: no `kid` on the
        // key, a set naming none, a `jwks_uri` we would have to fetch
        assert!(
            OAuthClient::from_registration_with_key(
                &registration(published("other")),
                crate::assertion::test_key(),
            )
            .is_ok()
        );
        for jwks in [
            serde_json::json!({ "keys": [{ "kty": "EC" }] }),
            serde_json::json!({ "keys": [] }),
            serde_json::Value::Null,
        ] {
            assert!(
                OAuthClient::from_registration_with_key(&registration(jwks.clone()), with_key_id())
                    .is_ok(),
                "refused {jwks}"
            );
        }
    }

    #[test]
    fn it_refuses_a_grant_the_registration_did_not_approve() {
        let registration = |grant_types: serde_json::Value| {
            serde_json::from_value::<volga_oauth_core::ClientRegistrationResponse>(
                serde_json::json!({
                    "client_id": "generated-id",
                    "client_secret": "generated-secret",
                    "grant_types": grant_types,
                }),
            )
            .unwrap()
        };

        let client = OAuthClient::from_registration(&registration(serde_json::json!([
            "client_credentials"
        ])))
        .unwrap();
        assert!(
            client
                .ensure_grant_registered(grant::CLIENT_CREDENTIALS)
                .is_ok()
        );

        let err = client
            .ensure_grant_registered(grant::JWT_BEARER)
            .unwrap_err();
        assert!(
            matches!(&err, ClientError::Validation(reason)
                if reason.contains("jwt-bearer") && reason.contains("client_credentials")),
            "got: {err}"
        );

        // ...the authorization code flow included, which this registration
        // did not approve either
        assert!(
            client
                .ensure_grant_registered(grant::AUTHORIZATION_CODE)
                .is_err()
        );

        // an omitted `grant_types` is not carte blanche: RFC 7591 Section 2
        // makes it authorization_code alone
        let defaulted =
            OAuthClient::from_registration(&registration(serde_json::json!([]))).unwrap();
        assert!(
            defaulted
                .ensure_grant_registered(grant::AUTHORIZATION_CODE)
                .is_ok()
        );
        assert!(
            defaulted
                .ensure_grant_registered(grant::CLIENT_CREDENTIALS)
                .is_err()
        );

        // refresh is the continuation of a grant already held, and servers
        // routinely omit it from a registration - never refused
        for approved in [
            serde_json::json!([]),
            serde_json::json!(["client_credentials"]),
        ] {
            let client = OAuthClient::from_registration(&registration(approved)).unwrap();
            assert!(client.ensure_grant_registered(grant::REFRESH_TOKEN).is_ok());
        }

        // a client that never went through a registration is unconstrained
        assert!(
            OAuthClient::new("my-client")
                .ensure_grant_registered(grant::JWT_BEARER)
                .is_ok()
        );
    }

    #[test]
    fn it_returns_send_token_endpoint_futures() {
        // the token-endpoint futures must be spawnable onto a multi-thread
        // runtime: nothing non-`Sync` (the form serializer) may be held
        // across their awaits
        fn assert_send(_: impl Send) {}

        let client = OAuthClient::new("my-client")
            .with_token_store(Arc::new(crate::InMemoryTokenStore::default()));
        let metadata = metadata();
        let request = client.authorization_request(&metadata).build().unwrap();

        assert_send(client.exchange_code(&metadata, "the-code", &request));
        assert_send(client.refresh(&metadata, "the-refresh-token"));
        assert_send(client.token("alice", &metadata));
    }

    #[test]
    fn it_validates_the_authorization_callback() {
        let client = OAuthClient::new("my-client");
        let metadata = metadata();
        let request = client.authorization_request(&metadata).build().unwrap();
        let state = request.state.clone();

        // no `iss` and no advertisement - nothing more to check
        assert!(request.validate_callback(&metadata, &state, None).is_ok());
        assert!(
            request
                .validate_callback(&metadata, &state, Some(&metadata.issuer))
                .is_ok()
        );

        // CSRF: a foreign `state` never reaches the token endpoint
        let err = request
            .validate_callback(&metadata, "forged", None)
            .unwrap_err();
        assert!(matches!(err, ClientError::Validation(reason) if reason.contains("state")));

        // mix-up: a present `iss` must match the issuer, advertised or not
        let err = request
            .validate_callback(&metadata, &state, Some("https://evil.example.com"))
            .unwrap_err();
        assert!(
            matches!(err, ClientError::Validation(reason) if reason.contains("`iss` mismatch"))
        );

        // ...and it is mandatory once the server advertises RFC 9207
        let advertised = metadata.with_authorization_response_iss_parameter(true);
        assert!(
            request
                .validate_callback(&advertised, &state, Some(&advertised.issuer))
                .is_ok()
        );
        let err = request
            .validate_callback(&advertised, &state, None)
            .unwrap_err();
        assert!(matches!(err, ClientError::Validation(reason) if reason.contains("RFC 9207")));
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

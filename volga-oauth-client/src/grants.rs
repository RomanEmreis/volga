//! Grants that authenticate the client itself
//!
//! The Authorization Code flow on [`OAuthClient`] covers a client acting
//! *for a user*. The grants here cover the machine-to-machine profiles,
//! where the client is the subject:
//!
//! * [`client_credentials`](OAuthClient::client_credentials) - RFC 6749
//!   Section 4.4, the client's own credentials are the whole grant.
//! * [`jwt_bearer`](OAuthClient::jwt_bearer) - RFC 7523 Section 2.1, a JWT
//!   the caller obtained elsewhere (a workload identity token, or an
//!   identity assertion from a prior exchange) is presented as the grant.
//! * [`exchange_token`](OAuthClient::exchange_token) - RFC 8693, one token
//!   is traded for another, possibly of a different type.
//!
//! All three are token requests: they authenticate with the configured
//! [`ClientAuthMethod`](crate::ClientAuthMethod) and share the transport
//! policy of [`ClientConfig`](crate::ClientConfig).

use std::time::{Duration, SystemTime};

use http::HeaderValue;
use serde::{Deserialize, Serialize};
use volga_oauth_core::AuthorizationServerMetadata;

use crate::{
    ClientError, OAuthClient, TokenSet,
    client::token_endpoint,
    token::{TokenResponse, expires_at},
};

/// The RFC 6749 Section 4.4 client credentials grant type
pub const GRANT_TYPE_CLIENT_CREDENTIALS: &str = "client_credentials";

/// The RFC 7523 Section 2.1 JWT bearer authorization grant type
pub const GRANT_TYPE_JWT_BEARER: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

/// The RFC 8693 token exchange grant type
pub const GRANT_TYPE_TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// RFC 8693 Section 3 token type identifier for an OAuth 2.0 access token
pub const TOKEN_TYPE_ACCESS_TOKEN: &str = "urn:ietf:params:oauth:token-type:access_token";

/// RFC 8693 Section 3 token type identifier for an OAuth 2.0 refresh token
pub const TOKEN_TYPE_REFRESH_TOKEN: &str = "urn:ietf:params:oauth:token-type:refresh_token";

/// RFC 8693 Section 3 token type identifier for an OpenID Connect ID token
pub const TOKEN_TYPE_ID_TOKEN: &str = "urn:ietf:params:oauth:token-type:id_token";

/// RFC 8693 Section 3 token type identifier for a plain JWT
pub const TOKEN_TYPE_JWT: &str = "urn:ietf:params:oauth:token-type:jwt";

/// Token type identifier of an identity assertion authorization grant,
/// the cross-domain assertion an identity provider issues for an
/// application to present to a resource's authorization server
pub const TOKEN_TYPE_ID_JAG: &str = "urn:ietf:params:oauth:token-type:id-jag";

impl OAuthClient {
    /// Requests a token with the client's own credentials (RFC 6749
    /// Section 4.4) - the machine-to-machine grant, with no user involved
    ///
    /// The client authenticates with whatever
    /// [`ClientAuthMethod`](crate::ClientAuthMethod) it was configured
    /// with; a public client has nothing to present and the request will
    /// be refused by any sane server.
    ///
    /// There is no authorization request to carry scopes here, so they go
    /// on the token request itself - or are omitted, leaving the server to
    /// apply the client's default grant.
    ///
    /// # Example
    /// ```no_run
    /// # use volga_oauth_client::{AuthorizationServerMetadata, ClientError, OAuthClient};
    /// # async fn run(metadata: &AuthorizationServerMetadata) -> Result<(), ClientError> {
    /// let client = OAuthClient::new("my-service").with_secret("s3cret");
    ///
    /// let tokens = client
    ///     .client_credentials(metadata)
    ///     .with_scopes(["inventory:read"])
    ///     .with_resource("https://api.example.com")
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn client_credentials<'a>(
        &'a self,
        metadata: &'a AuthorizationServerMetadata,
    ) -> ClientCredentialsRequest<'a> {
        ClientCredentialsRequest {
            request: TokenRequest::new(self, metadata, GRANT_TYPE_CLIENT_CREDENTIALS),
        }
    }

    /// Presents a JWT as an authorization grant (RFC 7523 Section 2.1)
    ///
    /// `assertion` is supplied by the caller rather than minted here: it
    /// is the token some other authority already issued - a workload
    /// identity token from the platform the client runs on, or an identity
    /// assertion obtained from a prior
    /// [`exchange_token`](Self::exchange_token).
    ///
    /// A failure is final: the assertion is either accepted or it is not,
    /// so do not retry it or fall back to another grant type - fix the
    /// assertion instead.
    ///
    /// # Example
    /// ```no_run
    /// # use volga_oauth_client::{AuthorizationServerMetadata, ClientError, OAuthClient};
    /// # async fn run(
    /// #     metadata: &AuthorizationServerMetadata,
    /// #     workload_jwt: &str,
    /// # ) -> Result<(), ClientError> {
    /// let tokens = OAuthClient::new("my-workload")
    ///     .jwt_bearer(metadata, workload_jwt)
    ///     .with_scopes(["inventory:read"])
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn jwt_bearer<'a>(
        &'a self,
        metadata: &'a AuthorizationServerMetadata,
        assertion: &'a str,
    ) -> JwtBearerRequest<'a> {
        let mut request = TokenRequest::new(self, metadata, GRANT_TYPE_JWT_BEARER);
        request.params.push(("assertion", assertion.to_owned()));
        JwtBearerRequest { request }
    }

    /// Trades one token for another (RFC 8693)
    ///
    /// `subject_token` is the token representing the party the new token
    /// is requested for, and `subject_token_type` the identifier of what
    /// it is - one of the `TOKEN_TYPE_*` constants, or any URI the server
    /// understands.
    ///
    /// Unlike the other grants this one does not necessarily yield a
    /// bearer access token, so it answers with an
    /// [`ExchangedToken`] carrying the `issued_token_type` the server
    /// decided on.
    ///
    /// # Example
    /// ```no_run
    /// # use volga_oauth_client::{
    /// #     AuthorizationServerMetadata, ClientError, OAuthClient, TOKEN_TYPE_ID_JAG,
    /// #     TOKEN_TYPE_ID_TOKEN,
    /// # };
    /// # async fn run(
    /// #     idp: &AuthorizationServerMetadata,
    /// #     id_token: &str,
    /// # ) -> Result<(), ClientError> {
    /// let client = OAuthClient::new("my-app").with_secret("s3cret");
    ///
    /// // exchange the user's ID token for an assertion the resource's
    /// // authorization server accepts...
    /// let exchanged = client
    ///     .exchange_token(idp, id_token, TOKEN_TYPE_ID_TOKEN)
    ///     .with_requested_token_type(TOKEN_TYPE_ID_JAG)
    ///     .with_audience("https://api.example.com")
    ///     .send()
    ///     .await?;
    ///
    /// // ...and present it there as a JWT bearer grant
    /// # let resource_server = idp;
    /// let tokens = client
    ///     .jwt_bearer(resource_server, &exchanged.token)
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn exchange_token<'a>(
        &'a self,
        metadata: &'a AuthorizationServerMetadata,
        subject_token: &'a str,
        subject_token_type: &'a str,
    ) -> TokenExchangeRequest<'a> {
        let mut request = TokenRequest::new(self, metadata, GRANT_TYPE_TOKEN_EXCHANGE);
        request.params.extend([
            ("subject_token", subject_token.to_owned()),
            ("subject_token_type", subject_token_type.to_owned()),
        ]);
        TokenExchangeRequest { request }
    }
}

/// The parts every token request in this module shares.
struct TokenRequest<'a> {
    client: &'a OAuthClient,
    metadata: &'a AuthorizationServerMetadata,
    grant_type: &'static str,
    params: Vec<(&'static str, String)>,
    scopes: Vec<String>,
    resources: Vec<String>,
    extra: Vec<(String, String)>,
}

impl<'a> TokenRequest<'a> {
    fn new(
        client: &'a OAuthClient,
        metadata: &'a AuthorizationServerMetadata,
        grant_type: &'static str,
    ) -> Self {
        Self {
            client,
            metadata,
            grant_type,
            params: Vec::new(),
            scopes: Vec::new(),
            resources: Vec::new(),
            extra: Vec::new(),
        }
    }

    /// Serializes the request body and derives the `Authorization` header
    /// the configured client authentication needs.
    ///
    /// The form serializer is not `Sync`: everything touching it happens
    /// here, so it is dropped before the caller awaits and the resulting
    /// future stays `Send`.
    fn build(&self) -> Result<(String, Option<HeaderValue>), ClientError> {
        ensure_grant_supported(self.metadata, self.grant_type)?;

        let mut form = form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", self.grant_type);

        for (name, value) in &self.params {
            form.append_pair(name, value);
        }

        if !self.scopes.is_empty() {
            form.append_pair("scope", &self.scopes.join(" "));
        }

        for resource in &self.resources {
            form.append_pair("resource", resource);
        }

        for (name, value) in &self.extra {
            form.append_pair(name, value);
        }

        let authorization = self.client.apply_client_auth(&mut form, self.metadata)?;
        Ok((form.finish(), authorization))
    }

    /// Sends the request and deserializes the response into `T`.
    async fn send<T: serde::de::DeserializeOwned>(&self) -> Result<T, ClientError> {
        let endpoint = token_endpoint(self.metadata)?;
        let (body, authorization) = self.build()?;
        self.client
            .post_token_request(endpoint, body, authorization)
            .await
    }
}

impl std::fmt::Debug for TokenRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenRequest")
            .field("grant_type", &self.grant_type)
            // the parameters carry assertions and subject tokens
            .field(
                "params",
                &self.params.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            )
            .field("scopes", &self.scopes)
            .field("resources", &self.resources)
            .field("extra", &self.extra)
            .finish_non_exhaustive()
    }
}

/// Adds the shared builder methods to a grant request wrapping a
/// [`TokenRequest`].
macro_rules! token_request_builder {
    ($ty:ident) => {
        impl $ty<'_> {
            /// Sets the requested scopes, joined into the `scope`
            /// parameter; omitted entirely when empty
            pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
            where
                I: IntoIterator<Item = S>,
                S: Into<String>,
            {
                self.request.scopes = scopes.into_iter().map(Into::into).collect();
                self
            }

            /// Adds a resource indicator (RFC 8707) naming the API the
            /// token is meant for, repeatable
            pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
                self.request.resources.push(resource.into());
                self
            }

            /// Adds an extra form parameter for server-specific
            /// extensions, repeatable
            pub fn with_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
                self.request.extra.push((name.into(), value.into()));
                self
            }
        }

        impl std::fmt::Debug for $ty<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple(stringify!($ty)).field(&self.request).finish()
            }
        }
    };
}

/// A client credentials request, created by
/// [`OAuthClient::client_credentials`]
#[must_use = "a token request does nothing until `send` is awaited"]
pub struct ClientCredentialsRequest<'a> {
    request: TokenRequest<'a>,
}

token_request_builder!(ClientCredentialsRequest);

impl ClientCredentialsRequest<'_> {
    /// Sends the request to the token endpoint
    ///
    /// Fails with [`ClientError::Validation`] when the metadata declares
    /// no `token_endpoint` or does not advertise `client_credentials`
    /// among its `grant_types_supported`, and with
    /// [`ClientError::Protocol`] when the server refuses the grant
    /// (`invalid_client`, `invalid_scope`, `unauthorized_client`).
    pub async fn send(self) -> Result<TokenSet, ClientError> {
        let response: TokenResponse = self.request.send().await?;
        Ok(response.into())
    }
}

/// A JWT bearer grant request, created by [`OAuthClient::jwt_bearer`]
#[must_use = "a token request does nothing until `send` is awaited"]
pub struct JwtBearerRequest<'a> {
    request: TokenRequest<'a>,
}

token_request_builder!(JwtBearerRequest);

impl JwtBearerRequest<'_> {
    /// Sends the request to the token endpoint
    ///
    /// Fails the same way [`ClientCredentialsRequest::send`] does; an
    /// `invalid_grant` here means the assertion itself was rejected and
    /// resending it will not help.
    pub async fn send(self) -> Result<TokenSet, ClientError> {
        let response: TokenResponse = self.request.send().await?;
        Ok(response.into())
    }
}

/// A token exchange request, created by [`OAuthClient::exchange_token`]
#[must_use = "a token request does nothing until `send` is awaited"]
pub struct TokenExchangeRequest<'a> {
    request: TokenRequest<'a>,
}

token_request_builder!(TokenExchangeRequest);

impl TokenExchangeRequest<'_> {
    /// Sets the `requested_token_type` - what the client wants back
    ///
    /// Defaults, per RFC 8693 Section 2.1, to whatever the server
    /// considers equivalent to an access token.
    pub fn with_requested_token_type(mut self, token_type: impl Into<String>) -> Self {
        self.request
            .params
            .push(("requested_token_type", token_type.into()));
        self
    }

    /// Sets the `audience` - the logical name of the relying party the
    /// issued token is meant for, repeatable
    ///
    /// Use [`with_resource`](Self::with_resource) instead when the target
    /// is identified by URI.
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.request.params.push(("audience", audience.into()));
        self
    }

    /// Sets the `actor_token` and its type - the party acting *on behalf
    /// of* the subject, for delegation semantics (RFC 8693 Section 2.1)
    pub fn with_actor_token(
        mut self,
        actor_token: impl Into<String>,
        actor_token_type: impl Into<String>,
    ) -> Self {
        self.request.params.extend([
            ("actor_token", actor_token.into()),
            ("actor_token_type", actor_token_type.into()),
        ]);
        self
    }

    /// Sends the request to the token endpoint
    ///
    /// Fails with [`ClientError::Validation`] when the metadata declares
    /// no `token_endpoint` or does not advertise the token exchange grant,
    /// and with [`ClientError::Protocol`] when the server refuses the
    /// exchange.
    pub async fn send(self) -> Result<ExchangedToken, ClientError> {
        let response: TokenExchangeResponse = self.request.send().await?;
        Ok(response.into())
    }
}

/// A successful token exchange response (RFC 8693 Section 2.2.1)
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenExchangeResponse {
    /// The issued token, whatever its type - RFC 8693 reuses the
    /// `access_token` member for it
    pub access_token: String,

    /// The type identifier of the issued token
    pub issued_token_type: String,

    /// How the token is to be presented, e.g. `Bearer` or `N_A` for a
    /// token that is not meant to be presented to an HTTP resource at all
    pub token_type: String,

    /// Token lifetime in seconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,

    /// The granted scope, when it differs from the requested one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Refresh token, when the server issued one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

/// The token an exchange produced
///
/// The counterpart of [`TokenSet`] for RFC 8693: the same absolute
/// expiration, plus the [`issued_token_type`](Self::issued_token_type) -
/// an exchange may hand back something other than a bearer access token,
/// and the client has to know which before using it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangedToken {
    /// The issued token
    pub token: String,

    /// The type identifier of the issued token, one of the
    /// `TOKEN_TYPE_*` constants or a server-specific URI
    pub issued_token_type: String,

    /// How the token is to be presented, e.g. `Bearer`
    pub token_type: String,

    /// The granted scope, when the server reported it
    pub scope: Option<String>,

    /// Refresh token, when the server issued one
    pub refresh_token: Option<String>,

    /// Absolute expiration; `None` when the server did not report a
    /// lifetime (or reported one too large to represent)
    pub expires_at: Option<SystemTime>,
}

impl ExchangedToken {
    /// Returns `true` when the issued token is a bearer token, and so
    /// usable as an `Authorization: Bearer` credential
    #[inline]
    pub fn is_bearer(&self) -> bool {
        self.token_type.eq_ignore_ascii_case("bearer")
    }

    /// Returns `true` when the token has expired
    ///
    /// A token without a known lifetime never reports as expired.
    #[inline]
    pub fn is_expired(&self) -> bool {
        self.expires_within(Duration::ZERO)
    }

    /// Returns `true` when the token expires within `leeway` from now
    /// (or already has)
    pub fn expires_within(&self, leeway: Duration) -> bool {
        self.expires_at.is_some_and(|expires_at| {
            SystemTime::now()
                .checked_add(leeway)
                .is_none_or(|deadline| deadline >= expires_at)
        })
    }
}

impl From<TokenExchangeResponse> for ExchangedToken {
    fn from(response: TokenExchangeResponse) -> Self {
        Self {
            token: response.access_token,
            issued_token_type: response.issued_token_type,
            token_type: response.token_type,
            scope: response.scope,
            refresh_token: response.refresh_token,
            expires_at: expires_at(response.expires_in),
        }
    }
}

impl std::fmt::Debug for TokenExchangeResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // tokens are credentials - never expose them in debug output
        f.debug_struct("TokenExchangeResponse")
            .field("access_token", &"[redacted]")
            .field("issued_token_type", &self.issued_token_type)
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

impl std::fmt::Debug for ExchangedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExchangedToken")
            .field("token", &"[redacted]")
            .field("issued_token_type", &self.issued_token_type)
            .field("token_type", &self.token_type)
            .field("scope", &self.scope)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Refuses a grant the server does not list in `grant_types_supported`.
///
/// The field is never empty in practice: RFC 8414 Section 2 defines the
/// default `["authorization_code", "implicit"]` for a metadata document
/// that omits it, and [`AuthorizationServerMetadata`] materializes that
/// default on deserialization. A server that supports these grants but
/// does not advertise them can still be used - push the grant type onto
/// the metadata value before handing it to the client.
fn ensure_grant_supported(
    metadata: &AuthorizationServerMetadata,
    grant_type: &str,
) -> Result<(), ClientError> {
    let supported = &metadata.grant_types_supported;
    if supported.is_empty() || supported.iter().any(|grant| grant == grant_type) {
        return Ok(());
    }

    Err(ClientError::validation(format!(
        "authorization server does not advertise the '{grant_type}' grant; \
         it supports {supported:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientAuthMethod, assertion::test_key};

    fn metadata() -> AuthorizationServerMetadata {
        let mut metadata = AuthorizationServerMetadata::new("https://auth.example.com");
        metadata.token_endpoint = Some("https://auth.example.com/token".into());
        metadata.grant_types_supported = vec![
            "authorization_code".into(),
            GRANT_TYPE_CLIENT_CREDENTIALS.into(),
            GRANT_TYPE_JWT_BEARER.into(),
            GRANT_TYPE_TOKEN_EXCHANGE.into(),
        ];
        metadata
    }

    fn pairs(body: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect()
    }

    fn value(body: &str, name: &str) -> Option<String> {
        pairs(body)
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    #[test]
    fn it_builds_a_client_credentials_request() {
        let client = OAuthClient::new("my-service")
            .with_secret("s3cret")
            .with_auth_method(ClientAuthMethod::Post);
        let metadata = metadata();

        let (body, authorization) = client
            .client_credentials(&metadata)
            .with_scopes(["read", "write"])
            .with_resource("https://api.example.com")
            .with_resource("https://other.example.com")
            .with_param("audience", "https://api.example.com")
            .request
            .build()
            .unwrap();

        assert!(authorization.is_none());
        assert_eq!(
            value(&body, "grant_type").as_deref(),
            Some("client_credentials")
        );
        assert_eq!(value(&body, "scope").as_deref(), Some("read write"));
        assert_eq!(
            value(&body, "audience").as_deref(),
            Some("https://api.example.com")
        );
        assert_eq!(value(&body, "client_secret").as_deref(), Some("s3cret"));
        let resources: Vec<_> = pairs(&body)
            .into_iter()
            .filter(|(key, _)| key == "resource")
            .map(|(_, value)| value)
            .collect();
        assert_eq!(
            resources,
            ["https://api.example.com", "https://other.example.com"]
        );

        // no scopes at all - the parameter is left out entirely
        let (body, _) = client
            .client_credentials(&metadata)
            .request
            .build()
            .unwrap();
        assert_eq!(value(&body, "scope"), None);
    }

    #[test]
    fn it_builds_a_jwt_bearer_request() {
        let client = OAuthClient::new("my-workload");
        let metadata = metadata();

        let (body, authorization) = client
            .jwt_bearer(&metadata, "the.workload.jwt")
            .with_scopes(["read"])
            .request
            .build()
            .unwrap();

        // the assertion is the credential: a public client sends no
        // secret, only its identifier
        assert!(authorization.is_none());
        assert_eq!(
            value(&body, "grant_type").as_deref(),
            Some(GRANT_TYPE_JWT_BEARER)
        );
        assert_eq!(
            value(&body, "assertion").as_deref(),
            Some("the.workload.jwt")
        );
        assert_eq!(value(&body, "client_id").as_deref(), Some("my-workload"));
        assert_eq!(value(&body, "scope").as_deref(), Some("read"));
    }

    #[test]
    fn it_builds_a_token_exchange_request() {
        let client = OAuthClient::new("my-app").with_private_key_jwt(test_key());
        let metadata = metadata();

        let (body, authorization) = client
            .exchange_token(&metadata, "the.id.token", TOKEN_TYPE_ID_TOKEN)
            .with_requested_token_type(TOKEN_TYPE_ID_JAG)
            .with_audience("https://api.example.com")
            .with_actor_token("the.actor.token", TOKEN_TYPE_JWT)
            .request
            .build()
            .unwrap();

        assert!(authorization.is_none());
        assert_eq!(
            value(&body, "grant_type").as_deref(),
            Some(GRANT_TYPE_TOKEN_EXCHANGE)
        );
        assert_eq!(
            value(&body, "subject_token").as_deref(),
            Some("the.id.token")
        );
        assert_eq!(
            value(&body, "subject_token_type").as_deref(),
            Some(TOKEN_TYPE_ID_TOKEN)
        );
        assert_eq!(
            value(&body, "requested_token_type").as_deref(),
            Some(TOKEN_TYPE_ID_JAG)
        );
        assert_eq!(
            value(&body, "audience").as_deref(),
            Some("https://api.example.com")
        );
        assert_eq!(
            value(&body, "actor_token").as_deref(),
            Some("the.actor.token")
        );
        assert_eq!(
            value(&body, "actor_token_type").as_deref(),
            Some(TOKEN_TYPE_JWT)
        );
        // ...authenticated by a freshly signed client assertion
        assert!(value(&body, "client_assertion").is_some());
    }

    #[test]
    fn it_refuses_a_grant_the_server_does_not_advertise() {
        let client = OAuthClient::new("my-service").with_secret("s3cret");

        // the RFC 8414 default a document without the field materializes
        // as covers neither of these grants
        let mut metadata = AuthorizationServerMetadata::new("https://auth.example.com");
        metadata.token_endpoint = Some("https://auth.example.com/token".into());

        let err = client
            .client_credentials(&metadata)
            .request
            .build()
            .unwrap_err();
        assert!(
            matches!(err, ClientError::Validation(reason) if reason.contains("client_credentials"))
        );

        let err = client
            .exchange_token(&metadata, "t", TOKEN_TYPE_JWT)
            .request
            .build()
            .unwrap_err();
        assert!(
            matches!(err, ClientError::Validation(reason) if reason.contains("token-exchange"))
        );

        // a server that advertises nothing at all is not second-guessed
        metadata.grant_types_supported.clear();
        assert!(client.client_credentials(&metadata).request.build().is_ok());
    }

    #[test]
    fn it_returns_send_token_endpoint_futures() {
        // the same `Send` requirement the authorization-code futures carry:
        // the form serializer must not be held across an await
        fn assert_send(_: impl Send) {}

        let client = OAuthClient::new("my-service").with_secret("s3cret");
        let metadata = metadata();

        assert_send(client.client_credentials(&metadata).send());
        assert_send(client.jwt_bearer(&metadata, "the.jwt").send());
        assert_send(
            client
                .exchange_token(&metadata, "the.jwt", TOKEN_TYPE_JWT)
                .send(),
        );
    }

    #[test]
    fn it_resolves_an_exchanged_token() {
        let response = TokenExchangeResponse {
            access_token: "issued".into(),
            issued_token_type: TOKEN_TYPE_ACCESS_TOKEN.into(),
            token_type: "Bearer".into(),
            expires_in: Some(3600),
            scope: Some("read".into()),
            refresh_token: None,
        };

        let token = ExchangedToken::from(response);
        assert_eq!(token.token, "issued");
        assert_eq!(token.issued_token_type, TOKEN_TYPE_ACCESS_TOKEN);
        assert!(token.is_bearer());
        assert!(!token.is_expired());
        assert!(token.expires_within(Duration::from_secs(3601)));

        // an exchange that yields something other than a bearer token
        let response: TokenExchangeResponse = serde_json::from_str(
            r#"{"access_token": "jag", "issued_token_type": "urn:ietf:params:oauth:token-type:id-jag", "token_type": "N_A"}"#,
        )
        .unwrap();
        let token = ExchangedToken::from(response);
        assert_eq!(token.issued_token_type, TOKEN_TYPE_ID_JAG);
        assert!(!token.is_bearer());
        assert_eq!(token.expires_at, None);
        assert!(!token.is_expired());
    }

    #[test]
    fn it_redacts_tokens_in_debug_output() {
        let response = TokenExchangeResponse {
            access_token: "s3cr3t-token".into(),
            issued_token_type: TOKEN_TYPE_ACCESS_TOKEN.into(),
            token_type: "Bearer".into(),
            expires_in: None,
            scope: None,
            refresh_token: Some("s3cr3t-refresh".into()),
        };
        let debug = format!("{response:?}");
        assert!(!debug.contains("s3cr3t") && debug.contains("[redacted]"));
        let debug = format!("{:?}", ExchangedToken::from(response));
        assert!(!debug.contains("s3cr3t") && debug.contains("[redacted]"));

        // a request must not leak the assertion it carries either
        let client = OAuthClient::new("my-workload");
        let metadata = metadata();
        let debug = format!("{:?}", client.jwt_bearer(&metadata, "the.workload.jwt"));
        assert!(!debug.contains("the.workload.jwt"));
        assert!(debug.contains("assertion"));
    }
}

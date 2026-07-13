//! Dynamic Client Registration client (RFC 7591)
//!
//! [`RegistrationClient`] submits [`ClientMetadata`] to an authorization
//! server's registration endpoint and returns the issued
//! [`ClientRegistrationResponse`]. Open registration works out of the box;
//! for servers requiring an initial access token, attach one with
//! [`with_initial_access_token`](RegistrationClient::with_initial_access_token).
//!
//! Limitations: the RFC 7592 management protocol (reading, updating and
//! deleting a registration) is not implemented — the
//! `registration_access_token` / `registration_client_uri` pair from the
//! response is surfaced for applications that need it. A
//! `software_statement` is passed through as-is; validating it is the
//! server's job.

use http::HeaderValue;

use volga_oauth_core::{AuthorizationServerMetadata, ClientMetadata, ClientRegistrationResponse};

use crate::{ClientConfig, ClientError, transport::Transport};

/// Client registering OAuth clients dynamically (RFC 7591)
///
/// # Example
/// ```no_run
/// use volga_oauth_client::{ClientMetadata, DiscoveryClient, OAuthClient, RegistrationClient};
///
/// # async fn register() -> Result<(), volga_oauth_client::ClientError> {
/// let metadata = DiscoveryClient::new()
///     .fetch_server_metadata("https://auth.example.com")
///     .await?;
///
/// let registered = RegistrationClient::new()
///     .register(
///         &metadata,
///         &ClientMetadata::new()
///             .with_redirect_uris(["https://app.example.com/callback"])
///             .with_client_name("My App"),
///     )
///     .await?;
///
/// // ready-to-use client under the issued credentials
/// let client = OAuthClient::from_registration(&registered)?;
/// # Ok(())
/// # }
/// ```
pub struct RegistrationClient {
    transport: Transport,
    initial_access_token: Option<String>,
}

impl std::fmt::Debug for RegistrationClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrationClient")
            .field("transport", &self.transport)
            .field(
                "initial_access_token",
                &self.initial_access_token.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

impl Default for RegistrationClient {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl RegistrationClient {
    /// Creates a registration client with the default [`ClientConfig`]
    pub fn new() -> Self {
        Self::with_config(ClientConfig::new())
    }

    /// Creates a registration client with the given configuration
    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            transport: Transport::new(config),
            initial_access_token: None,
        }
    }

    /// Attaches the initial access token sent as a `Bearer` credential
    /// with the registration request (RFC 7591 §3), for servers that do
    /// not allow open registration
    pub fn with_initial_access_token(mut self, token: impl Into<String>) -> Self {
        self.initial_access_token = Some(token.into());
        self
    }

    /// Registers a client at the `registration_endpoint` declared in
    /// `metadata` (RFC 7591 §3.1)
    pub async fn register(
        &self,
        metadata: &AuthorizationServerMetadata,
        request: &ClientMetadata,
    ) -> Result<ClientRegistrationResponse, ClientError> {
        let endpoint = metadata.registration_endpoint.as_deref().ok_or_else(|| {
            ClientError::validation("server metadata declares no registration_endpoint")
        })?;

        self.register_at(endpoint, request).await
    }

    /// Registers a client at an explicit registration endpoint URL
    pub async fn register_at(
        &self,
        endpoint: &str,
        request: &ClientMetadata,
    ) -> Result<ClientRegistrationResponse, ClientError> {
        validate_request(request)?;
        let body = serde_json::to_string(request)?;
        let authorization = self
            .initial_access_token
            .as_deref()
            .map(bearer_credentials)
            .transpose()?;

        let value = self
            .transport
            .post_json(endpoint, body, authorization)
            .await?;

        serde_json::from_value(value).map_err(Into::into)
    }
}

/// Client-side sanity checks per RFC 7591 §2 before any I/O.
fn validate_request(request: &ClientMetadata) -> Result<(), ClientError> {
    // a software statement is passed through opaquely and may itself
    // carry redirect_uris and grant_types (RFC 7591 §2.3), so the
    // redirect-based check below cannot be decided from the top-level
    // values alone — the server validates the signed metadata
    if request.software_statement.is_none() {
        // redirect_uris is REQUIRED for redirect-based grants; any declared
        // response type implies one too, since response types are delivered
        // via the redirection endpoint
        let redirect_based = request
            .grant_types
            .iter()
            .any(|grant| grant == "authorization_code" || grant == "implicit")
            // omitted grant_types default to authorization_code (§2)
            || request.grant_types.is_empty()
            || !request.response_types.is_empty();

        if redirect_based && request.redirect_uris.is_empty() {
            return Err(ClientError::validation(
                "redirect_uris is required for redirect-based grant types",
            ));
        }
    }

    if request.jwks.is_some() && request.jwks_uri.is_some() {
        return Err(ClientError::validation(
            "jwks and jwks_uri are mutually exclusive",
        ));
    }

    Ok(())
}

fn bearer_credentials(token: &str) -> Result<HeaderValue, ClientError> {
    HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| ClientError::validation("initial access token is not a valid header value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_validates_requests_before_any_io() {
        // default grant is authorization_code — redirect_uris required
        assert!(matches!(
            validate_request(&ClientMetadata::new()),
            Err(ClientError::Validation(reason)) if reason.contains("redirect_uris")
        ));
        assert!(matches!(
            validate_request(&ClientMetadata::default()),
            Err(ClientError::Validation(reason)) if reason.contains("redirect_uris")
        ));

        let valid = ClientMetadata::new().with_redirect_uris(["https://app.example.com/cb"]);
        assert!(validate_request(&valid).is_ok());

        // non-redirect grants need no redirect_uris (the builder clears
        // the default `code` response type for them)
        let client_credentials = ClientMetadata::new().with_grant_types(["client_credentials"]);
        assert!(client_credentials.response_types.is_empty());
        assert!(validate_request(&client_credentials).is_ok());

        // an explicitly declared response type still demands redirect_uris —
        // response types are delivered via the redirection endpoint
        let inconsistent = ClientMetadata::new()
            .with_grant_types(["client_credentials"])
            .with_response_types(["code"]);
        assert!(matches!(
            validate_request(&inconsistent),
            Err(ClientError::Validation(reason)) if reason.contains("redirect_uris")
        ));

        // a software statement may carry redirect_uris/grant_types inside
        // the signed JWT — the redirect-based check must not pre-reject it
        let signed_only = ClientMetadata::new().with_software_statement("a.b.c");
        assert!(validate_request(&signed_only).is_ok());

        let conflicting = valid
            .with_jwks_uri("https://app.example.com/jwks")
            .with_jwks(serde_json::json!({ "keys": [] }));
        assert!(matches!(
            validate_request(&conflicting),
            Err(ClientError::Validation(reason)) if reason.contains("mutually exclusive")
        ));
    }

    #[test]
    fn it_builds_bearer_credentials() {
        let header = bearer_credentials("initial-token").unwrap();
        assert_eq!(header.to_str().unwrap(), "Bearer initial-token");
        assert!(bearer_credentials("bad\ntoken").is_err());
    }

    #[test]
    fn it_redacts_the_initial_access_token_in_debug_output() {
        let client = RegistrationClient::new().with_initial_access_token("initial-token");
        let debug = format!("{client:?}");
        assert!(!debug.contains("initial-token"));
        assert!(debug.contains("[redacted]"));
    }
}

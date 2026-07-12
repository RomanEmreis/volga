//! Dynamic Client Registration models
//!
//! Serde models for OAuth 2.0 Dynamic Client Registration
//! ([RFC 7591](https://www.rfc-editor.org/rfc/rfc7591)): the client
//! metadata sent to the registration endpoint (§2) and the client
//! information response returned by it (§3.2.1).
//!
//! These are plain data types: submitting them (registration client) and
//! serving them (a registration endpoint) are built on top separately.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Client metadata submitted for registration per RFC 7591 §2
///
/// [`ClientMetadata::new`] prefills the OAuth 2.1 client profile
/// (`authorization_code` grant, `code` response type); extension and
/// OIDC-specific fields — including localized variants such as
/// `client_name#ja-JP` — are preserved in
/// [`additional_fields`](Self::additional_fields).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientMetadata {
    /// Redirection URIs for redirect-based flows; REQUIRED for clients
    /// using the `authorization_code` or `implicit` grants
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirect_uris: Vec<String>,

    /// Requested token endpoint authentication method
    /// (e.g. `client_secret_basic`, `client_secret_post`, `none`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<String>,

    /// Grant types the client will use
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grant_types: Vec<String>,

    /// Response types the client will use
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_types: Vec<String>,

    /// Human-readable client name shown to end users
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,

    /// URL of the client's home page
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<String>,

    /// URL of the client's logo
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_uri: Option<String>,

    /// Space-separated scope values the client will request
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Contact addresses for people responsible for the client
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contacts: Vec<String>,

    /// URL of the client's terms of service
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tos_uri: Option<String>,

    /// URL of the client's privacy policy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_uri: Option<String>,

    /// URL of the client's JWK Set document; mutually exclusive with
    /// [`jwks`](Self::jwks)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    /// The client's JWK Set document by value; mutually exclusive with
    /// [`jwks_uri`](Self::jwks_uri)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks: Option<serde_json::Value>,

    /// Identifier for the client software, stable across instances
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software_id: Option<String>,

    /// Version of the client software
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software_version: Option<String>,

    /// Software statement JWT asserting client metadata values (§2.3);
    /// issued by a third party and passed through as-is, not validated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software_statement: Option<String>,

    /// Extension and OIDC-specific fields not modeled above, including
    /// localized (`field#language-tag`) variants
    #[serde(flatten)]
    pub additional_fields: HashMap<String, serde_json::Value>,
}

impl ClientMetadata {
    /// Creates client metadata prefilled with the OAuth 2.1 profile:
    /// the `authorization_code` grant and the `code` response type
    pub fn new() -> Self {
        Self {
            grant_types: vec!["authorization_code".into()],
            response_types: vec!["code".into()],
            ..Self::default()
        }
    }

    /// Sets the redirection URIs
    pub fn with_redirect_uris<I, S>(mut self, uris: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.redirect_uris = uris.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the requested token endpoint authentication method
    pub fn with_token_endpoint_auth_method(mut self, method: impl Into<String>) -> Self {
        self.token_endpoint_auth_method = Some(method.into());
        self
    }

    /// Sets the grant types the client will use
    ///
    /// Response types only accompany redirect-based grants; when none of
    /// the given grants is redirect-based (`authorization_code` or
    /// `implicit`), the response types are cleared so the profile default
    /// `code` does not leak into e.g. a `client_credentials` registration
    /// (RFC 7591 §2 requires the two fields to be consistent). Set
    /// response types after grant types when an extension grant needs them.
    pub fn with_grant_types<I, S>(mut self, grant_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.grant_types = grant_types.into_iter().map(Into::into).collect();
        if !self
            .grant_types
            .iter()
            .any(|grant| grant == "authorization_code" || grant == "implicit")
        {
            self.response_types.clear();
        }
        self
    }

    /// Sets the response types the client will use
    pub fn with_response_types<I, S>(mut self, response_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.response_types = response_types.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the human-readable client name
    pub fn with_client_name(mut self, name: impl Into<String>) -> Self {
        self.client_name = Some(name.into());
        self
    }

    /// Sets the URL of the client's home page
    pub fn with_client_uri(mut self, uri: impl Into<String>) -> Self {
        self.client_uri = Some(uri.into());
        self
    }

    /// Sets the URL of the client's logo
    pub fn with_logo_uri(mut self, uri: impl Into<String>) -> Self {
        self.logo_uri = Some(uri.into());
        self
    }

    /// Sets the requested scopes, joined into the space-separated `scope`
    /// field
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let scopes: Vec<String> = scopes.into_iter().map(Into::into).collect();
        self.scope = Some(scopes.join(" "));
        self
    }

    /// Sets the contact addresses
    pub fn with_contacts<I, S>(mut self, contacts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.contacts = contacts.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the URL of the client's terms of service
    pub fn with_tos_uri(mut self, uri: impl Into<String>) -> Self {
        self.tos_uri = Some(uri.into());
        self
    }

    /// Sets the URL of the client's privacy policy
    pub fn with_policy_uri(mut self, uri: impl Into<String>) -> Self {
        self.policy_uri = Some(uri.into());
        self
    }

    /// Sets the URL of the client's JWK Set document
    pub fn with_jwks_uri(mut self, uri: impl Into<String>) -> Self {
        self.jwks_uri = Some(uri.into());
        self
    }

    /// Sets the client's JWK Set document by value
    pub fn with_jwks(mut self, jwks: impl Into<serde_json::Value>) -> Self {
        self.jwks = Some(jwks.into());
        self
    }

    /// Sets the software identifier
    pub fn with_software_id(mut self, id: impl Into<String>) -> Self {
        self.software_id = Some(id.into());
        self
    }

    /// Sets the software version
    pub fn with_software_version(mut self, version: impl Into<String>) -> Self {
        self.software_version = Some(version.into());
        self
    }

    /// Sets the software statement JWT (§2.3)
    pub fn with_software_statement(mut self, jwt: impl Into<String>) -> Self {
        self.software_statement = Some(jwt.into());
        self
    }

    /// Adds an extension or OIDC-specific field not modeled by the typed fields
    pub fn with_additional_field(
        mut self,
        name: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.additional_fields.insert(name.into(), value.into());
        self
    }
}

/// Client information response per RFC 7591 §3.2.1
///
/// Returned by the registration endpoint on success: the issued client
/// credentials plus all registered metadata (the server may have replaced
/// or extended the requested values — read them back from
/// [`metadata`](Self::metadata) rather than assuming the request was
/// stored verbatim). The `registration_access_token` /
/// `registration_client_uri` pair is issued by servers implementing the
/// RFC 7592 management protocol.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientRegistrationResponse {
    /// The issued client identifier
    pub client_id: String,

    /// The issued client secret, when the client is confidential
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// When `client_id` was issued, as seconds since the Unix epoch
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id_issued_at: Option<u64>,

    /// When `client_secret` expires as seconds since the Unix epoch,
    /// `0` meaning it never expires; REQUIRED when a secret is issued
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_expires_at: Option<u64>,

    /// Access token for the RFC 7592 client management endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_access_token: Option<String>,

    /// URL of the RFC 7592 client management endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_client_uri: Option<String>,

    /// The metadata as registered by the server
    #[serde(flatten)]
    pub metadata: ClientMetadata,
}

impl std::fmt::Debug for ClientRegistrationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // the secret and the management token are credentials — never
        // expose them in debug output
        f.debug_struct("ClientRegistrationResponse")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[redacted]"),
            )
            .field("client_id_issued_at", &self.client_id_issued_at)
            .field("client_secret_expires_at", &self.client_secret_expires_at)
            .field(
                "registration_access_token",
                &self
                    .registration_access_token
                    .as_ref()
                    .map(|_| "[redacted]"),
            )
            .field("registration_client_uri", &self.registration_client_uri)
            .field("metadata", &self.metadata)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn it_prefills_the_oauth21_profile() {
        let metadata = ClientMetadata::new();
        assert_eq!(metadata.grant_types, ["authorization_code"]);
        assert_eq!(metadata.response_types, ["code"]);
    }

    #[test]
    fn it_drops_response_types_for_non_redirect_grants() {
        let metadata = ClientMetadata::new().with_grant_types(["client_credentials"]);
        assert!(metadata.response_types.is_empty());
        // and the empty list is omitted from the wire document
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(json, json!({ "grant_types": ["client_credentials"] }));

        // a redirect-based grant in the set keeps the response types
        let metadata =
            ClientMetadata::new().with_grant_types(["authorization_code", "refresh_token"]);
        assert_eq!(metadata.response_types, ["code"]);

        // explicitly set response types afterwards are kept as-is
        let metadata = ClientMetadata::new()
            .with_grant_types(["urn:example:custom"])
            .with_response_types(["custom"]);
        assert_eq!(metadata.response_types, ["custom"]);
    }

    #[test]
    fn it_serializes_only_populated_fields() {
        let metadata = ClientMetadata::new()
            .with_redirect_uris(["https://app.example.com/callback"])
            .with_client_name("My App")
            .with_scopes(["read", "write"]);
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(
            json,
            json!({
                "redirect_uris": ["https://app.example.com/callback"],
                "grant_types": ["authorization_code"],
                "response_types": ["code"],
                "client_name": "My App",
                "scope": "read write"
            })
        );
    }

    #[test]
    fn it_preserves_extension_and_localized_fields() {
        let document = json!({
            "redirect_uris": ["https://app.example.com/callback"],
            "client_name": "My App",
            "client_name#ja-JP": "マイアプリ",
            "application_type": "web"
        });
        let metadata: ClientMetadata = serde_json::from_value(document.clone()).unwrap();
        assert_eq!(
            metadata.additional_fields["client_name#ja-JP"],
            json!("マイアプリ")
        );
        assert_eq!(metadata.additional_fields["application_type"], json!("web"));
        // lossless round-trip
        assert_eq!(serde_json::to_value(&metadata).unwrap(), document);
    }

    #[test]
    fn it_deserializes_a_registration_response() {
        let response: ClientRegistrationResponse = serde_json::from_value(json!({
            "client_id": "s6BhdRkqt3",
            "client_secret": "cf136dc3c1fc93f31185e5885805d",
            "client_id_issued_at": 2893256800u64,
            "client_secret_expires_at": 0,
            "registration_access_token": "this.is.an.access.token",
            "registration_client_uri": "https://server.example.com/register/s6BhdRkqt3",
            "redirect_uris": ["https://client.example.org/callback"],
            "grant_types": ["authorization_code", "refresh_token"],
            "client_name": "My Example Client",
            "token_endpoint_auth_method": "client_secret_basic"
        }))
        .unwrap();

        assert_eq!(response.client_id, "s6BhdRkqt3");
        assert_eq!(response.client_secret_expires_at, Some(0));
        assert_eq!(
            response.metadata.redirect_uris,
            ["https://client.example.org/callback"]
        );
        assert_eq!(
            response.metadata.token_endpoint_auth_method.as_deref(),
            Some("client_secret_basic")
        );
    }

    #[test]
    fn it_requires_a_client_id_in_the_response() {
        let result = serde_json::from_value::<ClientRegistrationResponse>(json!({
            "client_secret": "secret"
        }));
        assert!(result.is_err());
    }

    #[test]
    fn it_redacts_credentials_in_debug_output() {
        let response: ClientRegistrationResponse = serde_json::from_value(json!({
            "client_id": "s6BhdRkqt3",
            "client_secret": "s3cret-value",
            "registration_access_token": "management-token"
        }))
        .unwrap();
        let debug = format!("{response:?}");
        assert!(debug.contains("s6BhdRkqt3"));
        assert!(!debug.contains("s3cret-value"));
        assert!(!debug.contains("management-token"));
        assert!(debug.contains("[redacted]"));
    }
}

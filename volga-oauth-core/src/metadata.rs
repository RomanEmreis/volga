//! OAuth 2.0 metadata documents
//!
//! Serde models for Authorization Server Metadata
//! ([RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)) and Protected
//! Resource Metadata ([RFC 9728](https://www.rfc-editor.org/rfc/rfc9728)).
//!
//! These are plain data types: serving them (discovery handlers) and
//! fetching them (discovery client) are built on top separately.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Well-known path for OAuth 2.0 Authorization Server Metadata (RFC 8414)
pub const WELL_KNOWN_AUTHORIZATION_SERVER: &str = "/.well-known/oauth-authorization-server";

/// Well-known path for OpenID Connect Discovery metadata
pub const WELL_KNOWN_OPENID_CONFIGURATION: &str = "/.well-known/openid-configuration";

/// Well-known path for OAuth 2.0 Protected Resource Metadata (RFC 9728)
pub const WELL_KNOWN_PROTECTED_RESOURCE: &str = "/.well-known/oauth-protected-resource";

/// OAuth 2.0 Authorization Server Metadata per RFC 8414 §2
///
/// Also covers the OpenID Connect Discovery document, which shares this
/// format; OIDC-specific and other extension fields are preserved in
/// [`additional_fields`](Self::additional_fields).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    /// The authorization server's issuer identifier URL
    pub issuer: String,

    /// URL of the authorization endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<String>,

    /// URL of the token endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,

    /// URL of the server's JWK Set document
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    /// URL of the dynamic client registration endpoint (RFC 7591)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,

    /// Scope values supported by this server
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes_supported: Vec<String>,

    /// `response_type` values supported by this server
    ///
    /// REQUIRED per RFC 8414 §2: always serialized and must be present when
    /// deserializing a metadata document.
    pub response_types_supported: Vec<String>,

    /// `response_mode` values supported by this server
    ///
    /// When absent in a deserialized document, set to the RFC 8414 §2
    /// default `["query", "fragment"]`.
    #[serde(
        default = "default_response_modes",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub response_modes_supported: Vec<String>,

    /// Grant type values supported by this server
    ///
    /// When absent in a deserialized document, set to the RFC 8414 §2
    /// default `["authorization_code", "implicit"]`.
    #[serde(default = "default_grant_types", skip_serializing_if = "Vec::is_empty")]
    pub grant_types_supported: Vec<String>,

    /// Client authentication methods supported by the token endpoint
    ///
    /// When absent in a deserialized document, set to the RFC 8414 §2
    /// default `["client_secret_basic"]`.
    #[serde(
        default = "default_client_auth_methods",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub token_endpoint_auth_methods_supported: Vec<String>,

    /// JWS signing algorithms supported by the token endpoint for client authentication
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_endpoint_auth_signing_alg_values_supported: Vec<String>,

    /// URL of a page with human-readable developer documentation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_documentation: Option<String>,

    /// Languages supported for the user interface
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_locales_supported: Vec<String>,

    /// URL of the server's policy on client usage of registration data
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_policy_uri: Option<String>,

    /// URL of the server's terms of service
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_tos_uri: Option<String>,

    /// URL of the token revocation endpoint (RFC 7009)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_endpoint: Option<String>,

    /// Client authentication methods supported by the revocation endpoint
    ///
    /// When absent in a deserialized document, set to the RFC 8414 §2
    /// default `["client_secret_basic"]`.
    #[serde(
        default = "default_client_auth_methods",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub revocation_endpoint_auth_methods_supported: Vec<String>,

    /// JWS signing algorithms supported by the revocation endpoint for client authentication
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocation_endpoint_auth_signing_alg_values_supported: Vec<String>,

    /// URL of the token introspection endpoint (RFC 7662)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub introspection_endpoint: Option<String>,

    /// Client authentication methods supported by the introspection endpoint
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub introspection_endpoint_auth_methods_supported: Vec<String>,

    /// JWS signing algorithms supported by the introspection endpoint for client authentication
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub introspection_endpoint_auth_signing_alg_values_supported: Vec<String>,

    /// PKCE code challenge methods supported (e.g. `S256`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_challenge_methods_supported: Vec<String>,

    /// Whether the authorization response carries the `iss` parameter
    /// ([RFC 9207](https://www.rfc-editor.org/rfc/rfc9207) §3)
    ///
    /// Defaults to `false` — the RFC 9207 §3 default — and is then omitted
    /// from the serialized document. When `true`, clients must reject a
    /// callback that carries no `iss` (see
    /// `AuthorizationRequest::validate_callback` in `volga-oauth-client`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub authorization_response_iss_parameter_supported: bool,

    /// Extension and OIDC-specific fields not modeled above
    #[serde(flatten)]
    pub additional_fields: HashMap<String, serde_json::Value>,
}

impl AuthorizationServerMetadata {
    /// Creates a new metadata document for the given issuer identifier
    ///
    /// `response_types_supported` (REQUIRED per RFC 8414 §2) is prefilled
    /// with `["code"]` — the authorization code flow is the only
    /// redirect-based flow retained in OAuth 2.1. `grant_types_supported`
    /// is prefilled with `["authorization_code"]` to match: when this field
    /// is omitted, RFC 8414 clients assume the default
    /// `["authorization_code", "implicit"]`, which would wrongly advertise
    /// the implicit grant. Overwrite either field if the server supports a
    /// different set.
    pub fn new(issuer: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            response_types_supported: vec!["code".into()],
            grant_types_supported: vec!["authorization_code".into()],
            ..Default::default()
        }
    }

    /// Sets the authorization server's issuer identifier URL
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = issuer.into();
        self
    }

    /// Sets the URL of the authorization endpoint
    pub fn with_authorization_endpoint(mut self, url: impl Into<String>) -> Self {
        self.authorization_endpoint = Some(url.into());
        self
    }

    /// Sets the URL of the token endpoint
    pub fn with_token_endpoint(mut self, url: impl Into<String>) -> Self {
        self.token_endpoint = Some(url.into());
        self
    }

    /// Sets the URL of the server's JWK Set document
    pub fn with_jwks_uri(mut self, uri: impl Into<String>) -> Self {
        self.jwks_uri = Some(uri.into());
        self
    }

    /// Sets the URL of the dynamic client registration endpoint (RFC 7591)
    pub fn with_registration_endpoint(mut self, url: impl Into<String>) -> Self {
        self.registration_endpoint = Some(url.into());
        self
    }

    /// Sets the scope values supported by this server
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes_supported = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the `response_type` values supported by this server,
    /// replacing the `["code"]` prefilled by [`new`](Self::new)
    pub fn with_response_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.response_types_supported = types.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the `response_mode` values supported by this server
    pub fn with_response_modes<I, S>(mut self, modes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.response_modes_supported = modes.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the grant type values supported by this server, replacing the
    /// `["authorization_code"]` prefilled by [`new`](Self::new)
    pub fn with_grant_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.grant_types_supported = types.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the client authentication methods supported by the token endpoint
    pub fn with_token_endpoint_auth_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.token_endpoint_auth_methods_supported = methods.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the JWS signing algorithms supported by the token endpoint for
    /// client authentication
    pub fn with_token_endpoint_auth_signing_algs<I, S>(mut self, algs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.token_endpoint_auth_signing_alg_values_supported =
            algs.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the URL of a page with human-readable developer documentation
    pub fn with_service_documentation(mut self, url: impl Into<String>) -> Self {
        self.service_documentation = Some(url.into());
        self
    }

    /// Sets the languages supported for the user interface
    pub fn with_ui_locales<I, S>(mut self, locales: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.ui_locales_supported = locales.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the URL of the server's policy on client usage of registration data
    pub fn with_op_policy_uri(mut self, url: impl Into<String>) -> Self {
        self.op_policy_uri = Some(url.into());
        self
    }

    /// Sets the URL of the server's terms of service
    pub fn with_op_tos_uri(mut self, url: impl Into<String>) -> Self {
        self.op_tos_uri = Some(url.into());
        self
    }

    /// Sets the URL of the token revocation endpoint (RFC 7009)
    pub fn with_revocation_endpoint(mut self, url: impl Into<String>) -> Self {
        self.revocation_endpoint = Some(url.into());
        self
    }

    /// Sets the client authentication methods supported by the revocation endpoint
    pub fn with_revocation_endpoint_auth_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.revocation_endpoint_auth_methods_supported =
            methods.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the JWS signing algorithms supported by the revocation endpoint
    /// for client authentication
    pub fn with_revocation_endpoint_auth_signing_algs<I, S>(mut self, algs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.revocation_endpoint_auth_signing_alg_values_supported =
            algs.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the URL of the token introspection endpoint (RFC 7662)
    pub fn with_introspection_endpoint(mut self, url: impl Into<String>) -> Self {
        self.introspection_endpoint = Some(url.into());
        self
    }

    /// Sets the client authentication methods supported by the introspection endpoint
    pub fn with_introspection_endpoint_auth_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.introspection_endpoint_auth_methods_supported =
            methods.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the JWS signing algorithms supported by the introspection
    /// endpoint for client authentication
    pub fn with_introspection_endpoint_auth_signing_algs<I, S>(mut self, algs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.introspection_endpoint_auth_signing_alg_values_supported =
            algs.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the PKCE code challenge methods supported (e.g. `S256`)
    pub fn with_code_challenge_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.code_challenge_methods_supported = methods.into_iter().map(Into::into).collect();
        self
    }

    /// Advertises that authorization responses carry the `iss` parameter
    /// (RFC 9207 §3)
    ///
    /// Clients then treat a callback without `iss` as an error, which is
    /// what closes the mix-up attack the parameter exists to prevent.
    pub fn with_authorization_response_iss_parameter(mut self, supported: bool) -> Self {
        self.authorization_response_iss_parameter_supported = supported;
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

impl From<&str> for AuthorizationServerMetadata {
    #[inline]
    fn from(issuer: &str) -> Self {
        Self::new(issuer)
    }
}

impl From<String> for AuthorizationServerMetadata {
    #[inline]
    fn from(issuer: String) -> Self {
        Self::new(issuer)
    }
}

/// RFC 8414 §2 default for an omitted `response_modes_supported`
#[inline]
fn default_response_modes() -> Vec<String> {
    vec!["query".into(), "fragment".into()]
}

/// RFC 8414 §2 default for an omitted `grant_types_supported`
#[inline]
fn default_grant_types() -> Vec<String> {
    vec!["authorization_code".into(), "implicit".into()]
}

/// RFC 8414 §2 default for omitted token/revocation endpoint auth methods
#[inline]
fn default_client_auth_methods() -> Vec<String> {
    vec!["client_secret_basic".into()]
}

/// Keeps `false` — the spec default for the boolean metadata fields —
/// out of the serialized document
#[inline]
fn is_false(value: &bool) -> bool {
    !*value
}

/// OAuth 2.0 Protected Resource Metadata per RFC 9728 §2
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    /// The protected resource's resource identifier URL
    pub resource: String,

    /// Issuer identifiers of authorization servers that can be used with this resource
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authorization_servers: Vec<String>,

    /// URL of the protected resource's JWK Set document
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    /// Scope values used in authorization requests to access this resource
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes_supported: Vec<String>,

    /// Supported methods of sending a bearer token (`header`, `body`, `query`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bearer_methods_supported: Vec<String>,

    /// JWS signing algorithms supported for signed resource responses
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_signing_alg_values_supported: Vec<String>,

    /// Human-readable name of the protected resource
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,

    /// URL of a page with human-readable developer documentation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_documentation: Option<String>,

    /// URL of the resource's policy on client usage of data
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_policy_uri: Option<String>,

    /// URL of the resource's terms of service
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_tos_uri: Option<String>,

    /// Whether the resource supports mutual-TLS certificate-bound access tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_client_certificate_bound_access_tokens: Option<bool>,

    /// Authorization details type values supported (RFC 9396)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authorization_details_types_supported: Vec<String>,

    /// JWS algorithms supported for validating DPoP proof JWTs (RFC 9449)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dpop_signing_alg_values_supported: Vec<String>,

    /// Whether the resource always requires DPoP-bound access tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpop_bound_access_tokens_required: Option<bool>,

    /// Signed JWT containing the metadata itself
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_metadata: Option<String>,

    /// Extension fields not modeled above
    #[serde(flatten)]
    pub additional_fields: HashMap<String, serde_json::Value>,
}

impl ProtectedResourceMetadata {
    /// Creates a new metadata document for the given resource identifier
    pub fn new(resource: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            ..Default::default()
        }
    }

    /// Sets the protected resource's resource identifier URL
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = resource.into();
        self
    }

    /// Sets the issuer identifiers of authorization servers that can be
    /// used with this resource
    pub fn with_authorization_servers<I, S>(mut self, servers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.authorization_servers = servers.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the URL of the protected resource's JWK Set document
    pub fn with_jwks_uri(mut self, uri: impl Into<String>) -> Self {
        self.jwks_uri = Some(uri.into());
        self
    }

    /// Sets the scope values used in authorization requests to access
    /// this resource
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes_supported = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the supported methods of sending a bearer token
    /// (`header`, `body`, `query`)
    pub fn with_bearer_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.bearer_methods_supported = methods.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the JWS signing algorithms supported for signed resource responses
    pub fn with_resource_signing_algs<I, S>(mut self, algs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.resource_signing_alg_values_supported = algs.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the human-readable name of the protected resource
    pub fn with_resource_name(mut self, name: impl Into<String>) -> Self {
        self.resource_name = Some(name.into());
        self
    }

    /// Sets the URL of a page with human-readable developer documentation
    pub fn with_resource_documentation(mut self, url: impl Into<String>) -> Self {
        self.resource_documentation = Some(url.into());
        self
    }

    /// Sets the URL of the resource's policy on client usage of data
    pub fn with_resource_policy_uri(mut self, url: impl Into<String>) -> Self {
        self.resource_policy_uri = Some(url.into());
        self
    }

    /// Sets the URL of the resource's terms of service
    pub fn with_resource_tos_uri(mut self, url: impl Into<String>) -> Self {
        self.resource_tos_uri = Some(url.into());
        self
    }

    /// Sets whether the resource supports mutual-TLS certificate-bound
    /// access tokens
    #[inline]
    pub fn with_tls_client_certificate_bound_access_tokens(mut self, enabled: bool) -> Self {
        self.tls_client_certificate_bound_access_tokens = Some(enabled);
        self
    }

    /// Sets the authorization details type values supported (RFC 9396)
    pub fn with_authorization_details_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.authorization_details_types_supported = types.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the JWS algorithms supported for validating DPoP proof JWTs
    /// (RFC 9449)
    pub fn with_dpop_signing_algs<I, S>(mut self, algs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.dpop_signing_alg_values_supported = algs.into_iter().map(Into::into).collect();
        self
    }

    /// Sets whether the resource always requires DPoP-bound access tokens
    #[inline]
    pub fn with_dpop_bound_access_tokens(mut self, required: bool) -> Self {
        self.dpop_bound_access_tokens_required = Some(required);
        self
    }

    /// Sets the signed JWT containing the metadata itself
    pub fn with_signed_metadata(mut self, jwt: impl Into<String>) -> Self {
        self.signed_metadata = Some(jwt.into());
        self
    }

    /// Adds an extension field not modeled by the typed fields
    pub fn with_additional_field(
        mut self,
        name: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.additional_fields.insert(name.into(), value.into());
        self
    }
}

impl From<&str> for ProtectedResourceMetadata {
    #[inline]
    fn from(resource: &str) -> Self {
        Self::new(resource)
    }
}

impl From<String> for ProtectedResourceMetadata {
    #[inline]
    fn from(resource: String) -> Self {
        Self::new(resource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn it_serializes_minimal_server_metadata() {
        let metadata = AuthorizationServerMetadata::new("https://auth.example.com");
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(
            json,
            json!({
                "issuer": "https://auth.example.com",
                "response_types_supported": ["code"],
                "grant_types_supported": ["authorization_code"]
            })
        );
    }

    #[test]
    fn it_requires_response_types_when_deserializing_server_metadata() {
        let result: Result<AuthorizationServerMetadata, _> =
            serde_json::from_value(json!({ "issuer": "https://auth.example.com" }));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("response_types_supported")
        );
    }

    #[test]
    fn it_serializes_populated_server_metadata() {
        let mut metadata = AuthorizationServerMetadata::new("https://auth.example.com");
        metadata.authorization_endpoint = Some("https://auth.example.com/authorize".into());
        metadata.token_endpoint = Some("https://auth.example.com/token".into());
        metadata.response_types_supported = vec!["code".into()];
        metadata.code_challenge_methods_supported = vec!["S256".into()];

        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(
            json,
            json!({
                "issuer": "https://auth.example.com",
                "authorization_endpoint": "https://auth.example.com/authorize",
                "token_endpoint": "https://auth.example.com/token",
                "response_types_supported": ["code"],
                "grant_types_supported": ["authorization_code"],
                "code_challenge_methods_supported": ["S256"]
            })
        );
    }

    #[test]
    fn it_round_trips_the_iss_parameter_flag() {
        let metadata = AuthorizationServerMetadata::new("https://auth.example.com")
            .with_authorization_response_iss_parameter(true);
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(json["authorization_response_iss_parameter_supported"], true);

        let parsed: AuthorizationServerMetadata = serde_json::from_value(json).unwrap();
        assert!(parsed.authorization_response_iss_parameter_supported);
        // typed, not swept into the extension bag
        assert!(
            !parsed
                .additional_fields
                .contains_key("authorization_response_iss_parameter_supported")
        );

        // the RFC 9207 §3 default is `false`, and stays off the wire
        let metadata = AuthorizationServerMetadata::new("https://auth.example.com");
        assert!(!metadata.authorization_response_iss_parameter_supported);
        let json = serde_json::to_value(&metadata).unwrap();
        assert!(
            json.get("authorization_response_iss_parameter_supported")
                .is_none()
        );
    }

    #[test]
    fn it_roundtrips_server_metadata_with_extension_fields() {
        let doc = json!({
            "issuer": "https://auth.example.com",
            "token_endpoint": "https://auth.example.com/token",
            "response_types_supported": ["code", "id_token"],
            "userinfo_endpoint": "https://auth.example.com/userinfo",
            "claims_supported": ["sub", "email"]
        });

        let metadata: AuthorizationServerMetadata = serde_json::from_value(doc.clone()).unwrap();
        assert_eq!(metadata.issuer, "https://auth.example.com");
        assert_eq!(
            metadata.additional_fields.get("userinfo_endpoint"),
            Some(&json!("https://auth.example.com/userinfo"))
        );
        assert_eq!(
            metadata.additional_fields.get("claims_supported"),
            Some(&json!(["sub", "email"]))
        );

        // Omitted RFC-defaulted fields are materialized on the way out;
        // the result is semantically equivalent per RFC 8414 §2.
        let mut expected = doc;
        expected["response_modes_supported"] = json!(["query", "fragment"]);
        expected["grant_types_supported"] = json!(["authorization_code", "implicit"]);
        expected["token_endpoint_auth_methods_supported"] = json!(["client_secret_basic"]);
        expected["revocation_endpoint_auth_methods_supported"] = json!(["client_secret_basic"]);

        let back = serde_json::to_value(&metadata).unwrap();
        assert_eq!(back, expected);
    }

    #[test]
    fn it_applies_rfc_defaults_when_deserializing_server_metadata() {
        let metadata: AuthorizationServerMetadata = serde_json::from_value(json!({
            "issuer": "https://auth.example.com",
            "response_types_supported": ["code"]
        }))
        .unwrap();

        assert_eq!(metadata.response_modes_supported, ["query", "fragment"]);
        assert_eq!(
            metadata.grant_types_supported,
            ["authorization_code", "implicit"]
        );
        assert_eq!(
            metadata.token_endpoint_auth_methods_supported,
            ["client_secret_basic"]
        );
        assert_eq!(
            metadata.revocation_endpoint_auth_methods_supported,
            ["client_secret_basic"]
        );
    }

    #[test]
    fn it_keeps_explicit_values_over_rfc_defaults_when_deserializing() {
        let metadata: AuthorizationServerMetadata = serde_json::from_value(json!({
            "issuer": "https://auth.example.com",
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code"],
            "token_endpoint_auth_methods_supported": ["private_key_jwt"]
        }))
        .unwrap();

        assert_eq!(metadata.grant_types_supported, ["authorization_code"]);
        assert_eq!(
            metadata.token_endpoint_auth_methods_supported,
            ["private_key_jwt"]
        );
    }

    #[test]
    fn it_serializes_minimal_resource_metadata() {
        let metadata = ProtectedResourceMetadata::new("https://api.example.com");
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(json, json!({ "resource": "https://api.example.com" }));
    }

    #[test]
    fn it_roundtrips_resource_metadata() {
        let doc = json!({
            "resource": "https://api.example.com",
            "authorization_servers": ["https://auth.example.com"],
            "scopes_supported": ["read", "write"],
            "bearer_methods_supported": ["header"],
            "dpop_bound_access_tokens_required": false,
            "custom_extension": { "nested": true }
        });

        let metadata: ProtectedResourceMetadata = serde_json::from_value(doc.clone()).unwrap();
        assert_eq!(metadata.resource, "https://api.example.com");
        assert_eq!(
            metadata.authorization_servers,
            vec!["https://auth.example.com"]
        );
        assert_eq!(metadata.dpop_bound_access_tokens_required, Some(false));
        assert_eq!(
            metadata.additional_fields.get("custom_extension"),
            Some(&json!({ "nested": true }))
        );

        let back = serde_json::to_value(&metadata).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn it_builds_server_metadata_with_builder_methods() {
        let metadata = AuthorizationServerMetadata::new("https://old.example.com")
            .with_issuer("https://auth.example.com")
            .with_authorization_endpoint("https://auth.example.com/authorize")
            .with_token_endpoint("https://auth.example.com/token")
            .with_jwks_uri("https://auth.example.com/jwks")
            .with_registration_endpoint("https://auth.example.com/register")
            .with_scopes(["read", "write"])
            .with_response_types(["code", "id_token"])
            .with_response_modes(["query"])
            .with_grant_types(["authorization_code", "refresh_token"])
            .with_token_endpoint_auth_methods(["private_key_jwt"])
            .with_token_endpoint_auth_signing_algs(["ES256"])
            .with_service_documentation("https://auth.example.com/docs")
            .with_ui_locales(["en", "de"])
            .with_op_policy_uri("https://auth.example.com/policy")
            .with_op_tos_uri("https://auth.example.com/tos")
            .with_revocation_endpoint("https://auth.example.com/revoke")
            .with_revocation_endpoint_auth_methods(["client_secret_basic"])
            .with_revocation_endpoint_auth_signing_algs(["RS256"])
            .with_introspection_endpoint("https://auth.example.com/introspect")
            .with_introspection_endpoint_auth_methods(["client_secret_post"])
            .with_introspection_endpoint_auth_signing_algs(["PS256"])
            .with_code_challenge_methods(["S256"])
            .with_additional_field("subject_types_supported", json!(["public"]));

        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(
            json,
            json!({
                "issuer": "https://auth.example.com",
                "authorization_endpoint": "https://auth.example.com/authorize",
                "token_endpoint": "https://auth.example.com/token",
                "jwks_uri": "https://auth.example.com/jwks",
                "registration_endpoint": "https://auth.example.com/register",
                "scopes_supported": ["read", "write"],
                "response_types_supported": ["code", "id_token"],
                "response_modes_supported": ["query"],
                "grant_types_supported": ["authorization_code", "refresh_token"],
                "token_endpoint_auth_methods_supported": ["private_key_jwt"],
                "token_endpoint_auth_signing_alg_values_supported": ["ES256"],
                "service_documentation": "https://auth.example.com/docs",
                "ui_locales_supported": ["en", "de"],
                "op_policy_uri": "https://auth.example.com/policy",
                "op_tos_uri": "https://auth.example.com/tos",
                "revocation_endpoint": "https://auth.example.com/revoke",
                "revocation_endpoint_auth_methods_supported": ["client_secret_basic"],
                "revocation_endpoint_auth_signing_alg_values_supported": ["RS256"],
                "introspection_endpoint": "https://auth.example.com/introspect",
                "introspection_endpoint_auth_methods_supported": ["client_secret_post"],
                "introspection_endpoint_auth_signing_alg_values_supported": ["PS256"],
                "code_challenge_methods_supported": ["S256"],
                "subject_types_supported": ["public"]
            })
        );
    }

    #[test]
    fn it_builds_resource_metadata_with_builder_methods() {
        let metadata = ProtectedResourceMetadata::new("https://old.example.com")
            .with_resource("https://api.example.com")
            .with_authorization_servers(["https://auth.example.com"])
            .with_jwks_uri("https://api.example.com/jwks")
            .with_scopes(["read", "write"])
            .with_bearer_methods(["header"])
            .with_resource_signing_algs(["ES256"])
            .with_resource_name("Example API")
            .with_resource_documentation("https://api.example.com/docs")
            .with_resource_policy_uri("https://api.example.com/policy")
            .with_resource_tos_uri("https://api.example.com/tos")
            .with_tls_client_certificate_bound_access_tokens(true)
            .with_authorization_details_types(["payment_initiation"])
            .with_dpop_signing_algs(["ES256"])
            .with_dpop_bound_access_tokens(false)
            .with_signed_metadata("header.payload.signature")
            .with_additional_field("custom_extension", json!({ "nested": true }));

        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(
            json,
            json!({
                "resource": "https://api.example.com",
                "authorization_servers": ["https://auth.example.com"],
                "jwks_uri": "https://api.example.com/jwks",
                "scopes_supported": ["read", "write"],
                "bearer_methods_supported": ["header"],
                "resource_signing_alg_values_supported": ["ES256"],
                "resource_name": "Example API",
                "resource_documentation": "https://api.example.com/docs",
                "resource_policy_uri": "https://api.example.com/policy",
                "resource_tos_uri": "https://api.example.com/tos",
                "tls_client_certificate_bound_access_tokens": true,
                "authorization_details_types_supported": ["payment_initiation"],
                "dpop_signing_alg_values_supported": ["ES256"],
                "dpop_bound_access_tokens_required": false,
                "signed_metadata": "header.payload.signature",
                "custom_extension": { "nested": true }
            })
        );
    }

    #[test]
    fn it_exposes_well_known_paths() {
        assert_eq!(
            WELL_KNOWN_AUTHORIZATION_SERVER,
            "/.well-known/oauth-authorization-server"
        );
        assert_eq!(
            WELL_KNOWN_OPENID_CONFIGURATION,
            "/.well-known/openid-configuration"
        );
        assert_eq!(
            WELL_KNOWN_PROTECTED_RESOURCE,
            "/.well-known/oauth-protected-resource"
        );
    }
}

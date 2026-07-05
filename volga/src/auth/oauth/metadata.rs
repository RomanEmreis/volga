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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_modes_supported: Vec<String>,

    /// Grant type values supported by this server
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grant_types_supported: Vec<String>,

    /// Client authentication methods supported by the token endpoint
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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

    /// Extension and OIDC-specific fields not modeled above
    #[serde(flatten)]
    pub additional_fields: HashMap<String, serde_json::Value>,
}

impl AuthorizationServerMetadata {
    /// Creates a new metadata document for the given issuer identifier
    ///
    /// `response_types_supported` (REQUIRED per RFC 8414 §2) is prefilled
    /// with `["code"]` — the authorization code flow is the only
    /// redirect-based flow retained in OAuth 2.1. Overwrite the field if the
    /// server supports a different set.
    pub fn new(issuer: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            response_types_supported: vec!["code".into()],
            ..Default::default()
        }
    }
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
                "response_types_supported": ["code"]
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
                "code_challenge_methods_supported": ["S256"]
            })
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

        let back = serde_json::to_value(&metadata).unwrap();
        assert_eq!(back, doc);
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

//! OAuth discovery client
//!
//! Fetches Authorization Server Metadata (RFC 8414), Protected Resource
//! Metadata (RFC 9728) and OpenID Connect provider configuration over
//! HTTPS, with the semantic validation the specs require.

use serde::de::DeserializeOwned;
use std::sync::Arc;

use volga_oauth_core::{
    AuthorizationServerMetadata, ProtectedResourceMetadata, authorization_server_metadata_url,
    openid_configuration_url, protected_resource_metadata_url,
};

use crate::{ClientConfig, ClientError, MetadataCache, transport::Transport};

/// Client fetching OAuth discovery documents
///
/// The transport policy comes from [`ClientConfig`]: HTTPS is enforced by
/// default, requests carry a total timeout, redirects are followed up to a
/// configurable limit and response bodies above 1 MiB are rejected. Every
/// fetched document is validated against the identifier it was requested
/// for (RFC 8414 §3.3 / RFC 9728 §3.3) — including documents served from a
/// configured [`MetadataCache`].
///
/// # Example
/// ```no_run
/// use volga_oauth_client::DiscoveryClient;
///
/// # async fn discover() -> Result<(), volga_oauth_client::ClientError> {
/// let client = DiscoveryClient::new();
///
/// let resource = client
///     .fetch_resource_metadata("https://api.example.com")
///     .await?;
/// let server = client.discover_authorization_server(&resource).await?;
///
/// assert!(server.token_endpoint.is_some());
/// # Ok(())
/// # }
/// ```
pub struct DiscoveryClient {
    transport: Transport,
    cache: Option<Arc<dyn MetadataCache>>,
}

impl std::fmt::Debug for DiscoveryClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryClient")
            .field("transport", &self.transport)
            .field("cache", &self.cache.as_ref().map(|_| "dyn MetadataCache"))
            .finish()
    }
}

impl Default for DiscoveryClient {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoveryClient {
    /// Creates a discovery client with the default [`ClientConfig`]
    pub fn new() -> Self {
        Self::with_config(ClientConfig::new())
    }

    /// Creates a discovery client with the given configuration
    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            transport: Transport::new(config),
            cache: None,
        }
    }

    /// Attaches a [`MetadataCache`] consulted before each fetch and updated
    /// after each successful one
    pub fn with_cache(mut self, cache: Arc<dyn MetadataCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Fetches Authorization Server Metadata (RFC 8414 §3) for `issuer`
    ///
    /// The metadata URL is derived per RFC 8414 §3.1
    /// (`/.well-known/oauth-authorization-server` inserted between host and
    /// path). The `issuer` in the returned document must match the
    /// requested one (RFC 8414 §3.3); a mismatch fails with
    /// [`ClientError::Validation`].
    pub async fn fetch_server_metadata(
        &self,
        issuer: &str,
    ) -> Result<AuthorizationServerMetadata, ClientError> {
        let url = authorization_server_metadata_url(issuer)
            .map_err(|err| ClientError::validation(err.to_string()))?;

        let metadata: AuthorizationServerMetadata = self.fetch_document(&url).await?;
        validate_identifier("issuer", &metadata.issuer, issuer)?;

        Ok(metadata)
    }

    /// Fetches the OpenID Connect provider configuration for `issuer`
    ///
    /// Same document shape and the same issuer validation as
    /// [`fetch_server_metadata`](Self::fetch_server_metadata), but at the
    /// OIDC Discovery path (`/.well-known/openid-configuration` appended
    /// **after** the issuer's path).
    pub async fn fetch_oidc_metadata(
        &self,
        issuer: &str,
    ) -> Result<AuthorizationServerMetadata, ClientError> {
        let url = openid_configuration_url(issuer)
            .map_err(|err| ClientError::validation(err.to_string()))?;

        let metadata: AuthorizationServerMetadata = self.fetch_document(&url).await?;
        validate_identifier("issuer", &metadata.issuer, issuer)?;

        Ok(metadata)
    }

    /// Fetches Protected Resource Metadata (RFC 9728 §3) for `resource`
    ///
    /// The metadata URL is derived per RFC 9728 §3.1. The `resource` in the
    /// returned document must match the requested one (RFC 9728 §3.3).
    pub async fn fetch_resource_metadata(
        &self,
        resource: &str,
    ) -> Result<ProtectedResourceMetadata, ClientError> {
        let url = protected_resource_metadata_url(resource)
            .map_err(|err| ClientError::validation(err.to_string()))?;

        let metadata: ProtectedResourceMetadata = self.fetch_document(&url).await?;
        validate_identifier("resource", &metadata.resource, resource)?;

        Ok(metadata)
    }

    /// Fetches Protected Resource Metadata from an explicit URL — typically
    /// the `resource_metadata` parameter of a `WWW-Authenticate` challenge
    /// (RFC 9728 §5.1)
    ///
    /// When `expected_resource` is given, the document's `resource` must
    /// match it; pass `None` when the resource identifier is not known
    /// upfront and validate the returned value yourself.
    pub async fn fetch_resource_metadata_from_url(
        &self,
        url: &str,
        expected_resource: Option<&str>,
    ) -> Result<ProtectedResourceMetadata, ClientError> {
        let metadata: ProtectedResourceMetadata = self.fetch_document(url).await?;
        if let Some(expected) = expected_resource {
            validate_identifier("resource", &metadata.resource, expected)?;
        }

        Ok(metadata)
    }

    /// Discovers the authorization server protecting `resource_metadata`
    ///
    /// Takes the first advertised `authorization_servers` entry and fetches
    /// its metadata from the RFC 8414 path, falling back to the OIDC
    /// Discovery path when the former is not served (`404`) — authorization
    /// servers commonly publish only one of the two.
    pub async fn discover_authorization_server(
        &self,
        resource_metadata: &ProtectedResourceMetadata,
    ) -> Result<AuthorizationServerMetadata, ClientError> {
        let issuer = resource_metadata
            .authorization_servers
            .first()
            .ok_or_else(|| {
                ClientError::validation(
                    "resource metadata advertises no authorization servers".to_owned(),
                )
            })?;

        match self.fetch_server_metadata(issuer).await {
            Err(ClientError::Http(status)) if status == http::StatusCode::NOT_FOUND => {
                self.fetch_oidc_metadata(issuer).await
            }
            other => other,
        }
    }

    /// Fetches and deserializes a document, going through the cache when
    /// one is configured.
    async fn fetch_document<T: DeserializeOwned>(&self, url: &str) -> Result<T, ClientError> {
        if let Some(cache) = &self.cache
            && let Some(document) = cache.get(url)
        {
            return serde_json::from_value(document).map_err(Into::into);
        }

        let document = self.transport.get_json(url).await?;
        if let Some(cache) = &self.cache {
            cache.put(url, &document);
        }

        serde_json::from_value(document).map_err(Into::into)
    }
}

/// Compares a returned identifier against the requested one; RFC 8414
/// §3.3 / RFC 9728 §3.3 require the returned value to be **identical** to
/// the identifier used for discovery, so no normalization is applied — a
/// value differing only in case, default port or trailing slash still
/// binds to a distinct identifier and is rejected.
fn validate_identifier(field: &str, returned: &str, requested: &str) -> Result<(), ClientError> {
    if returned == requested {
        Ok(())
    } else {
        Err(ClientError::validation(format!(
            "{field} mismatch: requested '{requested}', document declares '{returned}'"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_requires_identical_identifiers() {
        assert!(
            validate_identifier(
                "issuer",
                "https://auth.example.com",
                "https://auth.example.com"
            )
            .is_ok()
        );
        // equivalent under URI normalization is still not identical
        assert!(
            validate_identifier(
                "issuer",
                "https://AUTH.example.com:443",
                "https://auth.example.com"
            )
            .is_err()
        );
        assert!(
            validate_identifier(
                "issuer",
                "https://auth.example.com/",
                "https://auth.example.com"
            )
            .is_err()
        );
        assert!(matches!(
            validate_identifier("issuer", "https://other.example.com", "https://auth.example.com"),
            Err(ClientError::Validation(reason)) if reason.contains("issuer mismatch")
        ));
    }
}

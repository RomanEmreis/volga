//! Internal HTTP transport
//!
//! A thin GET-only JSON client on top of `hyper-util`/`hyper-rustls`
//! applying the [`ClientConfig`] policy: HTTPS enforcement, a total
//! per-operation timeout, a bounded manual redirect loop (the legacy hyper
//! client does not follow redirects) and a response body size cap.

use bytes::Bytes;
use http::{
    Uri,
    header::{ACCEPT, LOCATION, USER_AGENT},
};
use http_body_util::{BodyExt, Empty, Limited};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde_json::Value;
use volga_oauth_core::OAuthError;

use crate::{ClientConfig, ClientError};

/// Maximum accepted response body size; metadata documents are small,
/// anything larger is rejected as malformed
const MAX_BODY_BYTES: usize = 1024 * 1024;

const USER_AGENT_VALUE: &str = concat!("volga-oauth-client/", env!("CARGO_PKG_VERSION"));

pub(crate) struct Transport {
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Empty<Bytes>>,
    config: ClientConfig,
}

impl Transport {
    pub(crate) fn new(config: ClientConfig) -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            // plain-http connections are still rejected by `check_scheme`
            // unless the config disables HTTPS enforcement
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(https);
        Self { client, config }
    }

    /// Fetches `url` with `GET` and parses the response body as JSON,
    /// applying the whole configured policy. The timeout covers the entire
    /// operation including redirects.
    pub(crate) async fn get_json(&self, url: &str) -> Result<Value, ClientError> {
        tokio::time::timeout(self.config.timeout(), self.get_json_inner(url))
            .await
            .map_err(|_| {
                ClientError::transport(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("request timed out after {:?}", self.config.timeout()),
                ))
            })?
    }

    async fn get_json_inner(&self, url: &str) -> Result<Value, ClientError> {
        let mut url = url.to_owned();
        let mut redirects = 0u8;
        loop {
            self.check_scheme(&url)?;
            let uri: Uri = url
                .parse()
                .map_err(|err| ClientError::validation(format!("invalid URL '{url}': {err}")))?;

            let req = http::Request::builder()
                .uri(uri.clone())
                .header(ACCEPT, "application/json")
                .header(USER_AGENT, USER_AGENT_VALUE)
                .body(Empty::new())
                .map_err(ClientError::transport)?;

            let res = self
                .client
                .request(req)
                .await
                .map_err(ClientError::transport)?;
            let status = res.status();

            if status.is_redirection() {
                redirects += 1;
                if redirects > self.config.max_redirects() {
                    return Err(ClientError::transport(format!(
                        "too many redirects (limit: {})",
                        self.config.max_redirects()
                    )));
                }
                let location = res
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| {
                        ClientError::validation(format!(
                            "redirect ({status}) without a valid Location header"
                        ))
                    })?;
                url = resolve_redirect(&uri, location)?;
                continue;
            }

            let bytes = Limited::new(res.into_body(), MAX_BODY_BYTES)
                .collect()
                .await
                .map_err(ClientError::transport)?
                .to_bytes();

            if !status.is_success() {
                // an OAuth error body (RFC 6749 §5.2) beats the bare status
                return match serde_json::from_slice::<OAuthError>(&bytes) {
                    Ok(err) => Err(err.into()),
                    Err(_) => Err(ClientError::Http(status)),
                };
            }
            return serde_json::from_slice(&bytes).map_err(Into::into);
        }
    }

    fn check_scheme(&self, url: &str) -> Result<(), ClientError> {
        if url.starts_with("https://") {
            Ok(())
        } else if url.starts_with("http://") {
            if self.config.enforce_https() {
                Err(ClientError::InsecureUrl(url.to_owned()))
            } else {
                Ok(())
            }
        } else {
            Err(ClientError::validation(format!(
                "unsupported URL scheme: '{url}'"
            )))
        }
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Resolves a `Location` header value against the URI being fetched;
/// absolute URLs are taken as-is, absolute paths inherit scheme and
/// authority. Other relative forms are rejected — metadata endpoints have
/// no business issuing them.
fn resolve_redirect(current: &Uri, location: &str) -> Result<String, ClientError> {
    if location.starts_with("https://") || location.starts_with("http://") {
        return Ok(location.to_owned());
    }
    if location.starts_with('/')
        && let (Some(scheme), Some(authority)) = (current.scheme_str(), current.authority())
    {
        return Ok(format!("{scheme}://{authority}{location}"));
    }
    Err(ClientError::validation(format!(
        "unsupported redirect location: '{location}'"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_resolves_redirect_locations() {
        let current: Uri = "https://auth.example.com/a/b".parse().unwrap();
        assert_eq!(
            resolve_redirect(&current, "https://other.example.com/x").unwrap(),
            "https://other.example.com/x"
        );
        assert_eq!(
            resolve_redirect(&current, "/x/y").unwrap(),
            "https://auth.example.com/x/y"
        );
        assert!(matches!(
            resolve_redirect(&current, "x/y"),
            Err(ClientError::Validation(_))
        ));
    }

    #[test]
    fn it_checks_url_schemes() {
        let strict = Transport::new(ClientConfig::new());
        assert!(strict.check_scheme("https://auth.example.com").is_ok());
        assert!(matches!(
            strict.check_scheme("http://auth.example.com"),
            Err(ClientError::InsecureUrl(_))
        ));
        assert!(matches!(
            strict.check_scheme("ftp://auth.example.com"),
            Err(ClientError::Validation(_))
        ));

        let relaxed = Transport::new(ClientConfig::new().require_https(false));
        assert!(relaxed.check_scheme("http://auth.example.com").is_ok());
    }

    #[test]
    fn it_prefers_oauth_error_body_over_status() {
        // exercised end-to-end in the integration tests; here we only pin
        // the parse rule the transport relies on
        let body = br#"{"error": "invalid_request", "error_description": "bad"}"#;
        let err: OAuthError = serde_json::from_slice(body).unwrap();
        assert_eq!(err.error.as_str(), "invalid_request");
        assert!(serde_json::from_slice::<OAuthError>(b"<html></html>").is_err());
        assert!(serde_json::from_slice::<OAuthError>(br#"{"message": "x"}"#).is_err());
    }
}

//! Internal HTTP transport
//!
//! A thin JSON client on top of `hyper-util`/`hyper-rustls` applying the
//! [`ClientConfig`] policy: HTTPS enforcement, a total per-operation
//! timeout, a bounded manual redirect loop for `GET` (the legacy hyper
//! client does not follow redirects) and a response body size cap. `POST`
//! is used for token-style form submissions and never follows redirects.

use bytes::Bytes;
use http::{
    HeaderMap, Method, StatusCode, Uri,
    header::{ACCEPT, CONTENT_TYPE, LOCATION, USER_AGENT},
};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde_json::Value;
use volga_oauth_core::OAuthError;

use crate::{ClientConfig, ClientError};

/// Maximum accepted response body size; metadata documents and token
/// responses are small, anything larger is rejected as malformed
const MAX_BODY_BYTES: usize = 1024 * 1024;

const USER_AGENT_VALUE: &str = concat!("volga-oauth-client/", env!("CARGO_PKG_VERSION"));

const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";

pub(crate) struct Transport {
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>,
    config: ClientConfig,
}

impl Transport {
    pub(crate) fn new(config: ClientConfig) -> Self {
        let builder = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            // plain-http connections are still rejected by `check_scheme`
            // unless the config disables HTTPS enforcement
            .https_or_http();

        // The HTTP version is negotiated via TLS ALPN from the enabled set
        #[cfg(all(feature = "http1", feature = "http2"))]
        let https = builder.enable_all_versions().build();
        #[cfg(all(feature = "http1", not(feature = "http2")))]
        let https = builder.enable_http1().build();
        #[cfg(all(feature = "http2", not(feature = "http1")))]
        let https = builder.enable_http2().build();

        let client = Client::builder(TokioExecutor::new());
        // Plaintext connections have no ALPN; in the HTTP/2-only build the
        // client must use prior knowledge (RFC 9113 Section 3.3) instead of
        // silently requiring TLS
        #[cfg(all(feature = "http2", not(feature = "http1")))]
        let client = {
            let mut client = client;
            client.http2_only(true);
            client
        };
        let client = client.build(https);
        Self { client, config }
    }

    /// Fetches `url` with `GET` and parses the response body as JSON,
    /// applying the whole configured policy. The timeout covers the entire
    /// operation including redirects.
    pub(crate) async fn get_json(&self, url: &str) -> Result<Value, ClientError> {
        tokio::time::timeout(self.config.timeout(), self.get_json_inner(url))
            .await
            .map_err(|_| self.timeout_error())?
    }

    /// Submits an `application/x-www-form-urlencoded` body to `url` with
    /// `POST` and returns the response for the caller to judge. Redirects
    /// are treated as errors - token-style endpoints have no business
    /// issuing them.
    ///
    /// `headers` are sent as given, which is how a request carries client
    /// authentication (`Authorization`) and any scheme-specific header
    /// alongside it.
    pub(crate) async fn post_form(
        &self,
        url: &str,
        body: String,
        headers: HeaderMap,
    ) -> Result<EndpointResponse, ClientError> {
        tokio::time::timeout(
            self.config.timeout(),
            self.post_inner(url, FORM_CONTENT_TYPE, body, headers),
        )
        .await
        .map_err(|_| self.timeout_error())?
    }

    /// Submits an `application/json` body to `url` with `POST`, under the
    /// same policy as [`post_form`](Self::post_form).
    pub(crate) async fn post_json(
        &self,
        url: &str,
        body: String,
        headers: HeaderMap,
    ) -> Result<EndpointResponse, ClientError> {
        tokio::time::timeout(
            self.config.timeout(),
            self.post_inner(url, "application/json", body, headers),
        )
        .await
        .map_err(|_| self.timeout_error())?
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
                .body(Full::default())
                .map_err(ClientError::transport)?;

            let res = self
                .client
                .request(req)
                .await
                .map_err(ClientError::transport)?;
            let status = res.status();

            if status.is_redirection() {
                // checking before incrementing keeps the counter within
                // `max_redirects`, so a limit of `u8::MAX` cannot overflow it
                if redirects == self.config.max_redirects() {
                    return Err(ClientError::transport(format!(
                        "too many redirects (limit: {})",
                        self.config.max_redirects()
                    )));
                }
                redirects += 1;
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

            return EndpointResponse::read(res).await?.into_json();
        }
    }

    async fn post_inner(
        &self,
        url: &str,
        content_type: &'static str,
        body: String,
        headers: HeaderMap,
    ) -> Result<EndpointResponse, ClientError> {
        self.check_scheme(url)?;

        let uri: Uri = url
            .parse()
            .map_err(|err| ClientError::validation(format!("invalid URL '{url}': {err}")))?;

        let mut builder = http::Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, content_type)
            .header(USER_AGENT, USER_AGENT_VALUE);

        if let Some(request_headers) = builder.headers_mut() {
            request_headers.extend(headers);
        }

        let req = builder
            .body(Full::new(Bytes::from(body)))
            .map_err(ClientError::transport)?;

        let res = self
            .client
            .request(req)
            .await
            .map_err(ClientError::transport)?;

        if res.status().is_redirection() {
            return Err(ClientError::validation(format!(
                "unexpected redirect ({}) from '{url}'",
                res.status()
            )));
        }

        EndpointResponse::read(res).await
    }

    pub(crate) fn check_scheme(&self, url: &str) -> Result<(), ClientError> {
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

    fn timeout_error(&self) -> ClientError {
        ClientError::transport(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("request timed out after {:?}", self.config.timeout()),
        ))
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// A response received from an OAuth endpoint, before its status is judged
///
/// [`into_json`](Self::into_json) applies the crate's usual rule and is
/// what every caller wants. The response is handed over whole because some
/// flows have to read a header off a *failed* response - RFC 9449
/// Section 8.2 answers `use_dpop_nonce` with the nonce to retry with in the
/// `DPoP-Nonce` header - which the error alone cannot carry.
pub(crate) struct EndpointResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl EndpointResponse {
    /// The response headers, available whatever the status says.
    ///
    /// Only DPoP reads them today - the nonce a `use_dpop_nonce` refusal
    /// carries lives in one (RFC 9449 Section 8.2), which the error the
    /// status turns into cannot hold.
    #[cfg_attr(not(feature = "dpop"), allow(dead_code))]
    pub(crate) fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns whether this response is a failure carrying the OAuth error
    /// `code` (RFC 6749 Section 5.2).
    ///
    /// The question a retryable refusal asks - `use_dpop_nonce` above all -
    /// before the response is turned into an error by
    /// [`into_json`](Self::into_json), which consumes it.
    #[cfg_attr(not(feature = "dpop"), allow(dead_code))]
    pub(crate) fn is_error(&self, code: &volga_oauth_core::OAuthErrorCode) -> bool {
        !self.status.is_success()
            && serde_json::from_slice::<OAuthError>(&self.body).is_ok_and(|err| &err.error == code)
    }

    /// Reads a non-redirect response, capturing the body up to
    /// [`MAX_BODY_BYTES`].
    async fn read(res: http::Response<Incoming>) -> Result<Self, ClientError> {
        let status = res.status();
        let headers = res.headers().clone();
        let body = Limited::new(res.into_body(), MAX_BODY_BYTES)
            .collect()
            .await
            .map_err(ClientError::transport)?
            .to_bytes();

        Ok(Self {
            status,
            headers,
            body,
        })
    }

    /// Turns the response into JSON: a success body is parsed as-is, an
    /// error body is parsed as an OAuth error (RFC 6749 Section 5.2) when
    /// possible and surfaced as the bare status otherwise.
    pub(crate) fn into_json(self) -> Result<Value, ClientError> {
        if !self.status.is_success() {
            // an OAuth error body (RFC 6749 Section 5.2) beats the bare status -
            // except on 404, which means the endpoint is not served at all:
            // no OAuth flow defines protocol errors for it, discovery keys
            // its OIDC fallback off that status, and frameworks commonly
            // attach JSON bodies to their catch-all 404
            if self.status != StatusCode::NOT_FOUND
                && let Ok(err) = serde_json::from_slice::<OAuthError>(&self.body)
            {
                return Err(err.into());
            }
            return Err(ClientError::Http(self.status));
        }

        serde_json::from_slice(&self.body).map_err(Into::into)
    }
}

/// Resolves a `Location` header value against the URI being fetched;
/// absolute URLs are taken as-is, scheme-relative URLs (`//host/path`)
/// inherit the scheme, absolute paths inherit scheme and authority. Other
/// relative forms are rejected - metadata endpoints have no business
/// issuing them.
fn resolve_redirect(current: &Uri, location: &str) -> Result<String, ClientError> {
    if location.starts_with("https://") || location.starts_with("http://") {
        return Ok(location.to_owned());
    }

    // a scheme-relative location designates its own authority (RFC 3986
    // Section 4.2) - it must not be mistaken for an absolute path below
    if let Some(authority_and_path) = location.strip_prefix("//") {
        return match current.scheme_str() {
            Some(scheme) => Ok(format!("{scheme}://{authority_and_path}")),
            None => Err(ClientError::validation(format!(
                "cannot resolve scheme-relative redirect '{location}'"
            ))),
        };
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
    use volga_oauth_core::OAuthErrorCode;

    fn response(status: u16, body: &str) -> EndpointResponse {
        let mut headers = HeaderMap::new();
        headers.insert("dpop-nonce", "the-nonce".parse().unwrap());
        EndpointResponse {
            status: StatusCode::from_u16(status).unwrap(),
            headers,
            body: Bytes::from(body.to_owned()),
        }
    }

    #[test]
    fn it_judges_a_response_by_status_and_body() {
        let ok = response(200, r#"{"access_token": "at"}"#);
        assert_eq!(ok.into_json().unwrap()["access_token"], "at");

        // an OAuth error body beats the bare status...
        let err = response(400, r#"{"error": "invalid_grant"}"#)
            .into_json()
            .unwrap_err();
        assert!(matches!(
            err,
            ClientError::Protocol(err) if err.error == OAuthErrorCode::InvalidGrant
        ));

        // ...except on 404, where the endpoint is simply not served
        let err = response(404, r#"{"error": "invalid_grant"}"#)
            .into_json()
            .unwrap_err();
        assert!(matches!(err, ClientError::Http(StatusCode::NOT_FOUND)));

        // a failure without a parseable OAuth body is the bare status,
        // and a success that is not JSON is a decode error
        let err = response(502, "<html></html>").into_json().unwrap_err();
        assert!(matches!(err, ClientError::Http(StatusCode::BAD_GATEWAY)));
        let err = response(200, "<html></html>").into_json().unwrap_err();
        assert!(matches!(err, ClientError::Decode(_)));
    }

    #[test]
    fn it_recognizes_a_retryable_refusal_before_consuming_it() {
        let failed = response(400, r#"{"error": "use_dpop_nonce"}"#);
        assert!(failed.is_error(&OAuthErrorCode::UseDpopNonce));
        assert!(!failed.is_error(&OAuthErrorCode::InvalidGrant));

        // a *successful* response is never an error, whatever its body
        // happens to look like
        assert!(
            !response(200, r#"{"error": "use_dpop_nonce"}"#)
                .is_error(&OAuthErrorCode::UseDpopNonce)
        );
        assert!(!response(400, "<html></html>").is_error(&OAuthErrorCode::UseDpopNonce));
    }

    #[test]
    fn it_keeps_the_headers_of_a_failed_response() {
        // the nonce a `use_dpop_nonce` refusal carries lives in a header,
        // so the headers must survive a status the body would turn into an
        // error (RFC 9449 Section 8.2)
        let failed = response(400, r#"{"error": "use_dpop_nonce"}"#);
        assert_eq!(failed.headers().get("dpop-nonce").unwrap(), "the-nonce");
        assert!(failed.into_json().is_err());
    }

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
        // scheme-relative: the advertised authority wins, the scheme is
        // inherited - not glued onto the current authority as a path
        assert_eq!(
            resolve_redirect(&current, "//other.example.com/x").unwrap(),
            "https://other.example.com/x"
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

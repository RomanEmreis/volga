//! Tools and utils for Bearer Token Authorization

use crate::auth::{Algorithm, DecodingKey, EncodingKey};
use crate::{
    HttpRequest,
    error::Error,
    headers::{AUTHORIZATION, Authorization, Header, HeaderMap, HeaderValue},
    http::{
        Extensions,
        endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload, Source},
        request_scope::HttpRequestScope,
    },
};
use futures_util::future::{Ready, ready};
use hyper::http::request::Parts;
use jsonwebtoken::{Header as JwtHeader, Validation};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashSet,
    fmt::{Display, Formatter},
    sync::Arc,
};

const SCHEME: &str = "Bearer ";

/// Bearer Token Authentication configuration
pub struct BearerAuthConfig {
    validation: Validation,
    encoding: Option<EncodingKey>,
    decoding: Option<DecodingKey>,
    strip_token_from_request: bool,
    strict_aud: Option<bool>,
    resources: Vec<String>,
    resource_metadata_url: Option<String>,
    require_https: bool,
}

impl Default for BearerAuthConfig {
    fn default() -> Self {
        Self {
            validation: Validation::default(),
            encoding: None,
            decoding: None,
            strip_token_from_request: true,
            strict_aud: None,
            resources: Vec::new(),
            resource_metadata_url: None,
            require_https: true,
        }
    }
}

impl std::fmt::Debug for BearerAuthConfig {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerAuthConfig")
            .field("validation", &"[redacted]")
            .field("encoding", &"[redacted]")
            .field("decoding", &"[redacted]")
            .field("strip_token_from_request", &self.strip_token_from_request)
            .field("strict_aud", &self.strict_aud)
            .field("resources", &self.resources)
            .field("resource_metadata_url", &self.resource_metadata_url)
            .field("require_https", &self.require_https)
            .finish()
    }
}

impl BearerAuthConfig {
    /// Specifies a security key to validate a JWT.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::DecodingKey};
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .set_decoding_key(DecodingKey::from_env("JWT_SECRET")));
    /// ```
    pub fn set_decoding_key(mut self, key: DecodingKey) -> Self {
        self.decoding = Some(key);
        self
    }

    /// Specifies a security key to generate a JWT.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::EncodingKey};
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .set_encoding_key(EncodingKey::from_env("JWT_SECRET")));
    /// ```
    pub fn set_encoding_key(mut self, key: EncodingKey) -> Self {
        self.encoding = Some(key);
        self
    }

    /// Specifies the algorithm supported for signing/verifying JWTs
    ///
    /// Default: [`Algorithm::HS256`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::Algorithm};
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .with_alg(Algorithm::RS256));
    /// ```
    pub fn with_alg(mut self, alg: Algorithm) -> Self {
        let jwt_alg: jsonwebtoken::Algorithm = alg.into();
        if !self.validation.algorithms.contains(&jwt_alg) {
            self.validation.algorithms.push(jwt_alg);
        }
        self
    }

    /// Sets one or more acceptable audience members
    ///
    /// An empty input is a no-op (no audience constraint or claim
    /// requirement is applied), so configurations sourced from runtime
    /// values that resolve to an empty list do not produce an
    /// unsatisfiable validation setup.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .with_aud(["some audience"]));
    /// ```
    pub fn with_aud<I, T>(mut self, aud: I) -> Self
    where
        T: ToString,
        I: AsRef<[T]>,
    {
        let aud = aud.as_ref();
        if aud.is_empty() {
            return self;
        }
        self.validation.set_audience(aud);
        self.apply_aud_required();
        self
    }

    /// Sets one or more acceptable issuers
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .with_iss(["some issuer"]));
    /// ```
    pub fn with_iss<I, T>(mut self, iss: I) -> Self
    where
        T: ToString,
        I: AsRef<[T]>,
    {
        self.validation.set_issuer(iss.as_ref());
        self
    }

    /// Specifies whether to validate the `aud` field or not.
    ///
    /// It will return an error if the `aud` field is not a member of the audience provided.
    /// Validation only happens if the `aud` claim is present in the token.
    ///
    /// Default: `true`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .validate_aud(true));
    /// ```
    pub fn validate_aud(mut self, validate: bool) -> Self {
        self.validation.validate_aud = validate;
        self
    }

    /// Specifies whether to validate the `exp` field or not.
    ///
    /// It will return an error if the time in the exp field is past.
    ///
    /// Default: `true`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .validate_exp(true));
    /// ```
    pub fn validate_exp(mut self, validate: bool) -> Self {
        self.validation.validate_exp = validate;
        self
    }

    /// Specifies whether to validate the `nbf` field or not.
    ///
    /// It will return an error if the current timestamp is before the time in the `nbf` field.
    /// Validation only happens if the `nbf` claim is present in the token.
    ///
    /// Default: `false`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .validate_nbf(true));
    /// ```
    pub fn validate_nbf(mut self, validate: bool) -> Self {
        self.validation.validate_nbf = validate;
        self
    }

    /// Returns a secret key to decode a JWT
    pub fn decoding_key(&self) -> Option<&DecodingKey> {
        self.decoding.as_ref()
    }

    /// Controls whether the `Authorization` header is removed from the request
    /// after successful bearer-token authentication.
    ///
    /// Defaults to `true`. Disable only if downstream handlers legitimately
    /// need the raw token (e.g., proxying to an upstream service).
    pub fn strip_token_from_request(mut self, enabled: bool) -> Self {
        self.strip_token_from_request = enabled;
        self
    }

    /// Requires the JWT to include an `aud` (audience) claim.
    ///
    /// When any audience is configured via [`with_aud`](Self::with_aud),
    /// [`with_resource`](Self::with_resource), or
    /// [`with_resources`](Self::with_resources), the `aud` claim is
    /// automatically required.
    ///
    /// Calling [`without_strict_aud`](Self::without_strict_aud)
    /// relaxes this requirement: tokens without an `aud` claim are accepted,
    /// but if the claim is present, its value is still validated.
    ///
    /// If no audience is configured, this setting has no effect - `aud`
    /// is not validated at all.
    pub fn with_strict_aud(mut self) -> Self {
        self.strict_aud = Some(true);
        self.apply_aud_required();
        self
    }

    /// Allows JWTs without an `aud` (audience) claim.
    ///
    /// The opt-out persists across builder call order: calling this before
    /// audiences are configured (e.g. before [`with_aud`](Self::with_aud) or
    /// [`with_resource`](Self::with_resource)) still suppresses the
    /// auto-required-`aud` behavior once audiences are added.
    ///
    /// See [`with_strict_aud`](Self::with_strict_aud) for details.
    pub fn without_strict_aud(mut self) -> Self {
        self.strict_aud = Some(false);
        self.apply_aud_required();
        self
    }

    /// Adds a single OAuth 2.0 resource indicator (RFC 8707).
    ///
    /// The URI is appended to the existing audience set (preserving any
    /// previously configured audiences) and `aud` is required in tokens.
    /// This is a semantic companion to [`with_aud`](Self::with_aud) that
    /// also records the URI as a resource for metadata/diagnostic purposes.
    pub fn with_resource<U: Into<String>>(mut self, uri: U) -> Self {
        let uri = uri.into();
        self.resources.push(uri.clone());
        self.validation
            .aud
            .get_or_insert_with(HashSet::new)
            .insert(uri);
        self.apply_aud_required();
        self
    }

    /// Adds multiple OAuth 2.0 resource indicators (RFC 8707).
    ///
    /// All URIs are appended to the existing audience set and `aud` is
    /// required in tokens. An empty iterator is a no-op (no audience
    /// constraint or claim requirement is applied).
    pub fn with_resources<I, U>(mut self, uris: I) -> Self
    where
        I: IntoIterator<Item = U>,
        U: Into<String>,
    {
        let new: Vec<String> = uris.into_iter().map(Into::into).collect();
        if new.is_empty() {
            return self;
        }
        self.resources.extend(new.iter().cloned());
        let aud = self.validation.aud.get_or_insert_with(HashSet::new);
        for uri in new {
            aud.insert(uri);
        }
        self.apply_aud_required();
        self
    }

    /// Sets the URL advertised as `resource_metadata` in the `WWW-Authenticate`
    /// challenge per RFC 9728 (OAuth 2.0 Protected Resource Metadata).
    ///
    /// volga does **not** serve the metadata document at this URL — that is
    /// the application's responsibility. This setting only controls the
    /// challenge-header hint sent to clients.
    pub fn with_resource_metadata_url<U: Into<String>>(mut self, url: U) -> Self {
        self.resource_metadata_url = Some(url.into());
        self
    }

    /// Requires the request to be received over TLS (HTTPS).
    ///
    /// When `true` (default), non-TLS requests are rejected with `400 Bad Request`
    /// unless the peer address is a loopback address (`127.0.0.0/8` or `::1`).
    /// Reverse-proxy deployments that terminate TLS upstream must disable this
    /// check; volga does not inspect `X-Forwarded-Proto`.
    pub fn require_https(mut self, enabled: bool) -> Self {
        self.require_https = enabled;
        self
    }

    /// Synchronizes the `aud` entry in `required_spec_claims` with the
    /// configured audience set and the user's `strict_aud` preference.
    ///
    /// - When no audiences are configured, `aud` is never required: marking
    ///   it required without any accepted values yields an unsatisfiable
    ///   validation (every token would fail).
    /// - Otherwise, `aud` is required iff `strict_aud` is unset (default-on)
    ///   or explicitly `Some(true)`.
    #[inline]
    fn apply_aud_required(&mut self) {
        if self.validation.aud.is_none() {
            self.validation.required_spec_claims.remove("aud");
            return;
        }
        if self.strict_aud.unwrap_or(true) {
            self.validation.required_spec_claims.insert("aud".into());
        } else {
            self.validation.required_spec_claims.remove("aud");
        }
    }
}

/// Service that handles bearer token generation and validation
#[derive(Clone)]
pub struct BearerTokenService {
    validation: Arc<Validation>,
    encoding: Option<Arc<EncodingKey>>,
    decoding: Option<Arc<DecodingKey>>,
    pub(crate) strip_token_from_request: bool,
    pub(crate) resource_metadata_url: Option<Arc<str>>,
    pub(crate) require_https: bool,
    pub(crate) tls_enabled: bool,
}

impl std::fmt::Debug for BearerTokenService {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerTokenService")
            .field("validation", &"[redacted]")
            .field("encoding", &"[redacted]")
            .field("decoding", &"[redacted]")
            .field("strip_token_from_request", &self.strip_token_from_request)
            .field("resource_metadata_url", &self.resource_metadata_url)
            .field("require_https", &self.require_https)
            .field("tls_enabled", &self.tls_enabled)
            .finish()
    }
}

impl From<BearerAuthConfig> for BearerTokenService {
    #[inline]
    fn from(value: BearerAuthConfig) -> Self {
        Self::from_config(value, false)
    }
}

impl BearerTokenService {
    /// Builds a [`BearerTokenService`] from a [`BearerAuthConfig`], recording
    /// whether the outer server accepts TLS traffic.
    ///
    /// `tls_enabled` is consulted together with `require_https` to decide
    /// whether to reject plaintext requests. Applications should prefer the
    /// [`From<BearerAuthConfig>`] impl in tests and when TLS status is irrelevant.
    #[inline]
    pub(crate) fn from_config(cfg: BearerAuthConfig, tls_enabled: bool) -> Self {
        Self {
            validation: Arc::new(cfg.validation),
            encoding: cfg.encoding.map(Arc::new),
            decoding: cfg.decoding.map(Arc::new),
            strip_token_from_request: cfg.strip_token_from_request,
            resource_metadata_url: cfg.resource_metadata_url.map(Into::into),
            require_https: cfg.require_https,
            tls_enabled,
        }
    }

    /// Returns a secret key for decoding JWT
    pub fn decoding_key(&self) -> Option<Arc<DecodingKey>> {
        self.decoding.clone()
    }

    /// Returns a secret key for encoding JWT
    pub fn encoding_key(&self) -> Option<Arc<EncodingKey>> {
        self.encoding.clone()
    }

    /// Encode the header and claims given and sign the payload using the algorithm from the header and the key.
    /// If the algorithm given is RSA or EC, the key needs to be in the PEM format.
    pub fn encode<C: Serialize>(&self, claims: &C) -> Result<Bearer, Error> {
        let Some(encoding_key) = &self.encoding else {
            return Err(Error::server_error("Missing security key"));
        };
        jsonwebtoken::encode(&JwtHeader::default(), claims, &encoding_key.0)
            .map_err(Error::from)
            .map(|s| Bearer(s.into()))
    }

    /// Decodes and validates a JSON Web Token
    ///
    /// If the token or its signature is invalid or the claims fail validation, it will return an error.
    pub fn decode<C: DeserializeOwned + Clone>(&self, bearer: Bearer) -> Result<C, Error> {
        let Some(decoding_key) = &self.decoding else {
            return Err(Error::server_error("Missing security key"));
        };
        jsonwebtoken::decode(&*bearer.0, &decoding_key.0, &self.validation)
            .map_err(Error::from)
            .map(|t| t.claims)
    }
}

/// Wraps a bearer token string
#[derive(Clone)]
pub struct Bearer(Arc<str>);

impl std::fmt::Debug for Bearer {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Bearer").field(&"[redacted]").finish()
    }
}

impl TryFrom<&HeaderValue> for Bearer {
    type Error = Error;

    #[inline]
    fn try_from(header: &HeaderValue) -> Result<Self, Self::Error> {
        let token = header.to_str().map_err(Error::from)?;
        let token = token
            .strip_prefix(SCHEME)
            .map(str::trim)
            .ok_or_else(|| Error::client_error("Header: Missing Credentials"))?;
        Ok(Self(token.into()))
    }
}

impl TryFrom<Header<Authorization>> for Bearer {
    type Error = Error;

    #[inline]
    fn try_from(header: Header<Authorization>) -> Result<Self, Self::Error> {
        let header = header.into_inner();
        Self::try_from(&header)
    }
}

impl TryFrom<&HeaderMap> for Bearer {
    type Error = Error;

    #[inline]
    fn try_from(headers: &HeaderMap) -> Result<Self, Self::Error> {
        let header = headers
            .get(AUTHORIZATION)
            .ok_or_else(|| Error::client_error("Header: Missing Authorization header"))?;
        header.try_into()
    }
}

impl TryFrom<&Parts> for Bearer {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Self::Error> {
        Self::try_from(&parts.headers)
    }
}

impl Display for Bearer {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromRequestParts for Bearer {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Self::try_from(parts)
    }
}

impl FromRequestRef for Bearer {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Self::try_from(req.headers())
    }
}

impl FromPayload for Bearer {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else {
            unreachable!()
        };
        ready(Self::from_parts(parts))
    }
}

impl TryFrom<&Extensions> for BearerTokenService {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        extensions
            .get::<HttpRequestScope>()
            .and_then(|s| s.bearer_token_service.clone())
            .ok_or_else(|| {
                Error::server_error("Bearer Token authorization is not properly configured")
            })
    }
}

impl FromRequestParts for BearerTokenService {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Self::try_from(&parts.extensions)
    }
}

impl FromRequestRef for BearerTokenService {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Self::try_from(req.extensions())
    }
}

impl FromPayload for BearerTokenService {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else {
            unreachable!()
        };
        ready(Self::from_parts(parts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{Algorithm, DecodingKey, EncodingKey};
    use crate::headers::{HeaderMap, HeaderValue};
    use crate::http::Extensions;
    use hyper::Request;
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        exp: u64,
        aud: String,
        iss: String,
    }

    const SECRET: &[u8] = b"test_secret_key";

    #[tokio::test]
    async fn it_tests_bearer_auth_config_default() {
        let config = BearerAuthConfig::default();

        assert!(
            config
                .validation
                .algorithms
                .contains(&jsonwebtoken::Algorithm::HS256)
        );
        assert!(config.encoding.is_none());
        assert!(config.decoding.is_none());
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_set_decoding_key() {
        let decoding_key = DecodingKey::from_secret(SECRET);
        let config = BearerAuthConfig::default().set_decoding_key(decoding_key);

        assert!(config.decoding.is_some());
        assert!(config.decoding_key().is_some());
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_set_encoding_key() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let config = BearerAuthConfig::default().set_encoding_key(encoding_key);

        assert!(config.encoding.is_some());
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_with_alg() {
        let config = BearerAuthConfig::default()
            .with_alg(Algorithm::HS512)
            .with_alg(Algorithm::RS256);

        assert!(
            config
                .validation
                .algorithms
                .contains(&jsonwebtoken::Algorithm::HS256)
        );
        assert!(
            config
                .validation
                .algorithms
                .contains(&jsonwebtoken::Algorithm::HS512)
        );
        assert!(
            config
                .validation
                .algorithms
                .contains(&jsonwebtoken::Algorithm::RS256)
        );
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_with_aud() {
        let audience = vec!["test-audience", "another-audience"];
        let config = BearerAuthConfig::default().with_aud(&audience);

        assert!(config.validation.aud.is_some());
        let expected_aud: HashSet<String> = audience.iter().map(|s| s.to_string()).collect();
        assert_eq!(config.validation.aud.unwrap(), expected_aud);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_with_iss() {
        let issuers = vec!["test-issuer", "another-issuer"];
        let config = BearerAuthConfig::default().with_iss(&issuers);

        assert!(config.validation.iss.is_some());
        let expected_iss: HashSet<String> = issuers.iter().map(|s| s.to_string()).collect();
        assert_eq!(config.validation.iss.unwrap(), expected_iss);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_validate_aud() {
        let config = BearerAuthConfig::default().validate_aud(false);

        assert!(!config.validation.validate_aud);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_validate_exp() {
        let config = BearerAuthConfig::default().validate_exp(false);

        assert!(!config.validation.validate_exp);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_validate_nbf() {
        let config = BearerAuthConfig::default().validate_nbf(true);

        assert!(config.validation.validate_nbf);
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_from_config() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let decoding_key = DecodingKey::from_secret(SECRET);

        let config = BearerAuthConfig::default()
            .set_encoding_key(encoding_key)
            .set_decoding_key(decoding_key)
            .with_alg(Algorithm::HS256);

        let service: BearerTokenService = config.into();

        assert!(service.encoding_key().is_some());
        assert!(service.decoding_key().is_some());
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_encode_success() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let config = BearerAuthConfig::default().set_encoding_key(encoding_key);
        let service: BearerTokenService = config.into();

        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;

        let claims = TestClaims {
            sub: "test".to_string(),
            exp,
            aud: "test-audience".to_string(),
            iss: "test-issuer".to_string(),
        };

        let result = service.encode(&claims);
        assert!(result.is_ok());

        let bearer = result.unwrap();
        assert!(!bearer.to_string().is_empty());
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_encode_missing_key() {
        let config = BearerAuthConfig::default();
        let service: BearerTokenService = config.into();

        let claims = TestClaims {
            sub: "test".to_string(),
            exp: 0,
            aud: "test-audience".to_string(),
            iss: "test-issuer".to_string(),
        };

        let result = service.encode(&claims);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing security key")
        );
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_decode_success() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let decoding_key = DecodingKey::from_secret(SECRET);

        let config = BearerAuthConfig::default()
            .set_encoding_key(encoding_key)
            .set_decoding_key(decoding_key)
            .with_aud(["test-audience"])
            .with_iss(["test-issuer"]);
        let service: BearerTokenService = config.into();

        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;

        let original_claims = TestClaims {
            sub: "test".to_string(),
            exp,
            aud: "test-audience".to_string(),
            iss: "test-issuer".to_string(),
        };

        let bearer = service.encode(&original_claims).unwrap();
        let decoded_claims: TestClaims = service.decode(bearer).unwrap();

        assert_eq!(original_claims.sub, decoded_claims.sub);
        assert_eq!(original_claims.exp, decoded_claims.exp);
        assert_eq!(original_claims.aud, decoded_claims.aud);
        assert_eq!(original_claims.iss, decoded_claims.iss);
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_decode_missing_key() {
        let config = BearerAuthConfig::default();
        let service: BearerTokenService = config.into();

        let bearer = Bearer("invalid.token.here".into());
        let result: Result<TestClaims, Error> = service.decode(bearer);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing security key")
        );
    }

    #[tokio::test]
    async fn it_tests_bearer_from_header_value_success() {
        let token_value = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.test";
        let header_value = HeaderValue::from_str(&format!("Bearer {token_value}")).unwrap();

        let bearer = Bearer::try_from(&header_value).unwrap();
        assert_eq!(bearer.to_string(), token_value);
    }

    #[tokio::test]
    async fn it_tests_bearer_from_header_value_missing_bearer() {
        let header_value = HeaderValue::from_static("InvalidScheme token");

        let result = Bearer::try_from(&header_value);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing Credentials")
        );
    }

    #[tokio::test]
    async fn it_tests_bearer_from_header_value_invalid_utf8() {
        let header_value = HeaderValue::from_static("Bearer valid-token");
        let result = Bearer::try_from(&header_value);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_tests_bearer_from_header_map_success() {
        let mut headers = HeaderMap::new();
        let token_value = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.test";
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token_value}")).unwrap(),
        );

        let bearer = Bearer::try_from(&headers).unwrap();
        assert_eq!(bearer.to_string(), token_value);
    }

    #[tokio::test]
    async fn it_tests_bearer_from_header_map_missing_header() {
        let headers = HeaderMap::new();

        let result = Bearer::try_from(&headers);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing Authorization header")
        );
    }

    #[tokio::test]
    async fn it_tests_bearer_from_parts_success() {
        let token_value = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.test";

        let req = Request::builder()
            .header(AUTHORIZATION, format!("Bearer {token_value}"))
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        let bearer = Bearer::try_from(&parts).unwrap();
        assert_eq!(bearer.to_string(), token_value);
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_from_extensions_success() {
        use crate::http::request_scope::HttpRequestScope;
        let service = BearerTokenService::from(BearerAuthConfig::default());
        let mut extensions = Extensions::new();
        extensions.insert(HttpRequestScope {
            bearer_token_service: Some(service.clone()),
            ..HttpRequestScope::default()
        });

        let result = BearerTokenService::try_from(&extensions);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_from_extensions_missing() {
        let extensions = Extensions::new();

        let result = BearerTokenService::try_from(&extensions);
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("Bearer Token authorization is not properly configured")
        );
    }

    #[tokio::test]
    async fn it_tests_bearer_display() {
        let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.test";
        let bearer = Bearer(token.into());

        assert_eq!(format!("{bearer}"), token);
    }

    #[tokio::test]
    async fn it_tests_bearer_token_with_whitespace() {
        let token_value = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.test";
        let header_value = HeaderValue::from_str(&format!("Bearer   {token_value}   ")).unwrap();

        let bearer = Bearer::try_from(&header_value).unwrap();
        assert_eq!(bearer.to_string(), token_value);
    }

    #[tokio::test]
    async fn it_tests_encode_decode_round_trip_with_validation() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let decoding_key = DecodingKey::from_secret(SECRET);

        let config = BearerAuthConfig::default()
            .set_encoding_key(encoding_key)
            .set_decoding_key(decoding_key)
            .with_aud(vec!["test-audience"])
            .with_iss(vec!["test-issuer"])
            .validate_aud(true);

        let service: BearerTokenService = config.into();

        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;

        let claims = TestClaims {
            sub: "test".to_string(),
            exp,
            aud: "test-audience".to_string(),
            iss: "test-issuer".to_string(),
        };

        let bearer = service.encode(&claims).unwrap();
        let decoded: TestClaims = service.decode(bearer).unwrap();

        assert_eq!(claims.sub, decoded.sub);
        assert_eq!(claims.aud, decoded.aud);
        assert_eq!(claims.iss, decoded.iss);
    }

    #[test]
    fn it_debugs_bearer() {
        let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.test";
        let bearer = Bearer(token.into());

        assert_eq!(format!("{bearer:?}"), r#"Bearer("[redacted]")"#);
    }

    #[test]
    fn it_debugs_bearer_auth_config() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let decoding_key = DecodingKey::from_secret(SECRET);

        let config = BearerAuthConfig::default()
            .set_encoding_key(encoding_key)
            .set_decoding_key(decoding_key)
            .with_aud(vec!["test-audience"])
            .with_iss(vec!["test-issuer"])
            .validate_aud(true);

        assert_eq!(
            format!("{config:?}"),
            r#"BearerAuthConfig { validation: "[redacted]", encoding: "[redacted]", decoding: "[redacted]", strip_token_from_request: true, strict_aud: None, resources: [], resource_metadata_url: None, require_https: true }"#
        );
    }

    #[test]
    fn it_debugs_bearer_token_service_config() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let decoding_key = DecodingKey::from_secret(SECRET);

        let config = BearerAuthConfig::default()
            .set_encoding_key(encoding_key)
            .set_decoding_key(decoding_key)
            .with_aud(vec!["test-audience"])
            .with_iss(vec!["test-issuer"])
            .validate_aud(true);

        let service: BearerTokenService = config.into();

        assert_eq!(
            format!("{service:?}"),
            r#"BearerTokenService { validation: "[redacted]", encoding: "[redacted]", decoding: "[redacted]", strip_token_from_request: true, resource_metadata_url: None, require_https: true, tls_enabled: false }"#
        );
    }

    #[tokio::test]
    async fn it_defaults_strip_token_true() {
        let config = BearerAuthConfig::default();
        assert!(config.strip_token_from_request);
    }

    #[tokio::test]
    async fn it_allows_disabling_strip_token() {
        let config = BearerAuthConfig::default().strip_token_from_request(false);
        assert!(!config.strip_token_from_request);
    }

    #[tokio::test]
    async fn it_propagates_strip_token_to_service() {
        let service: BearerTokenService = BearerAuthConfig::default()
            .strip_token_from_request(false)
            .into();
        assert!(!service.strip_token_from_request);
    }

    #[tokio::test]
    async fn it_does_not_require_aud_by_default() {
        let config = BearerAuthConfig::default();
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_auto_enables_strict_aud_when_with_aud_called() {
        let config = BearerAuthConfig::default().with_aud(["test-aud"]);
        assert!(config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_respects_explicit_strict_aud_disabling() {
        let config = BearerAuthConfig::default()
            .with_aud(["test-aud"])
            .without_strict_aud();
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_strict_aud_noop_without_audiences() {
        let config = BearerAuthConfig::default().with_strict_aud();
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_stores_single_resource() {
        let config = BearerAuthConfig::default().with_resource("https://api.example.com/");
        assert_eq!(
            config.resources,
            vec!["https://api.example.com/".to_string()]
        );
        assert!(config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_stores_multiple_resources() {
        let config = BearerAuthConfig::default()
            .with_resources(["https://api.a.example/", "https://api.b.example/"]);
        assert_eq!(config.resources.len(), 2);
        assert!(config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_preserves_existing_audiences_when_adding_resource() {
        let config = BearerAuthConfig::default()
            .with_aud(["existing-aud"])
            .with_resource("https://api.example.com/");
        let aud = config.validation.aud.expect("aud should be set");
        assert!(aud.contains("existing-aud"));
        assert!(aud.contains("https://api.example.com/"));
    }

    #[tokio::test]
    async fn it_preserves_existing_audiences_when_adding_resources() {
        let config = BearerAuthConfig::default()
            .with_aud(["existing-aud"])
            .with_resources(["https://api.a.example/", "https://api.b.example/"]);
        let aud = config.validation.aud.expect("aud should be set");
        assert!(aud.contains("existing-aud"));
        assert!(aud.contains("https://api.a.example/"));
        assert!(aud.contains("https://api.b.example/"));
    }

    #[tokio::test]
    async fn it_accumulates_resources_across_calls() {
        let config = BearerAuthConfig::default()
            .with_resource("https://api.a.example/")
            .with_resource("https://api.b.example/");
        let aud = config.validation.aud.expect("aud should be set");
        assert!(aud.contains("https://api.a.example/"));
        assert!(aud.contains("https://api.b.example/"));
    }

    #[tokio::test]
    async fn it_treats_empty_resources_iter_as_noop() {
        let config = BearerAuthConfig::default().with_resources(Vec::<String>::new());
        assert!(config.resources.is_empty());
        assert!(config.validation.aud.is_none());
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_treats_empty_with_aud_as_noop() {
        let config = BearerAuthConfig::default().with_aud(Vec::<String>::new());
        assert!(config.validation.aud.is_none());
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_persists_strict_aud_optout_before_with_aud() {
        let config = BearerAuthConfig::default()
            .without_strict_aud()
            .with_aud(["test-aud"]);
        assert!(config.validation.aud.is_some());
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_persists_strict_aud_optout_before_with_resource() {
        let config = BearerAuthConfig::default()
            .without_strict_aud()
            .with_resource("https://api.example.com/");
        assert!(config.validation.aud.is_some());
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_persists_strict_aud_optout_before_with_resources() {
        let config = BearerAuthConfig::default()
            .without_strict_aud()
            .with_resources(["https://api.a.example/", "https://api.b.example/"]);
        assert!(config.validation.aud.is_some());
        assert!(!config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_persists_strict_aud_optin_across_calls() {
        let config = BearerAuthConfig::default()
            .with_strict_aud()
            .with_aud(["test-aud"]);
        assert!(config.validation.required_spec_claims.contains("aud"));
    }

    #[tokio::test]
    async fn it_stores_resource_metadata_url() {
        let config = BearerAuthConfig::default().with_resource_metadata_url(
            "https://api.example.com/.well-known/oauth-protected-resource",
        );
        assert_eq!(
            config.resource_metadata_url.as_deref(),
            Some("https://api.example.com/.well-known/oauth-protected-resource")
        );
    }

    #[tokio::test]
    async fn it_defaults_require_https_true() {
        let config = BearerAuthConfig::default();
        assert!(config.require_https);
    }

    #[tokio::test]
    async fn it_allows_disabling_require_https() {
        let config = BearerAuthConfig::default().require_https(false);
        assert!(!config.require_https);
    }

    #[tokio::test]
    async fn it_propagates_require_https_to_service() {
        let service: BearerTokenService = BearerAuthConfig::default().require_https(false).into();
        assert!(!service.require_https);
    }

    #[tokio::test]
    async fn it_from_config_records_tls_enabled() {
        let service = BearerTokenService::from_config(BearerAuthConfig::default(), true);
        assert!(service.tls_enabled);
    }

    #[tokio::test]
    async fn it_from_impl_defaults_tls_disabled() {
        let service: BearerTokenService = BearerAuthConfig::default().into();
        assert!(!service.tls_enabled);
    }
}

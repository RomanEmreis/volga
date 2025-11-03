//! Tools and utils for Bearer Token Authorization

use std::{fmt::{Display, Formatter}, sync::Arc};
use futures_util::future::{ready, Ready};
use hyper::http::request::Parts;
use serde::{de::DeserializeOwned, Serialize};
use crate::{
    http::{Extensions, endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload, Source}}, 
    headers::{HeaderMap, HeaderValue, Header, Authorization, AUTHORIZATION},
    error::Error, 
    HttpRequest,
};
use jsonwebtoken::{
    EncodingKey, DecodingKey,
    Validation, Algorithm, 
    Header as JwtHeader
};

const SCHEME: &str = "Bearer ";

/// Bearer Token Authentication configuration
#[derive(Default)]
pub struct BearerAuthConfig {
    validation: Validation,
    encoding: Option<EncodingKey>,
    decoding: Option<DecodingKey>,
}

impl std::fmt::Debug for BearerAuthConfig {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerAuthConfig")
            .field("validation", &"[redacted]")
            .field("encoding", &"[redacted]")
            .field("decoding", &"[redacted]")
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
    /// let secret = std::env::var("JWT_SECRET")
    ///     .expect("JWT_SECRET must be set");
    /// 
    /// let key = DecodingKey::from_secret(secret.as_bytes());
    /// 
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .set_decoding_key(key));
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
    /// let secret = std::env::var("JWT_SECRET")
    ///     .expect("JWT_SECRET must be set");
    /// 
    /// let key = EncodingKey::from_secret(secret.as_bytes());
    /// 
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth
    ///         .set_encoding_key(key));
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
        if !self.validation.algorithms.contains(&alg) { 
            self.validation.algorithms.push(alg);  
        }
        self
    }
    
    /// Sets one or more acceptable audience members
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
        I: AsRef<[T]>
    {
        self.validation.set_audience(aud.as_ref());
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
        I: AsRef<[T]>
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

    /// Retuuns a ecret key to decode a JWT
    pub fn decoding_key(&self) -> Option<&DecodingKey> {
        self.decoding.as_ref()
    }
}

/// Service that handles bearer token generation and validation
#[derive(Clone)]
pub struct BearerTokenService {
    validation: Arc<Validation>,
    encoding: Option<Arc<EncodingKey>>,
    decoding: Option<Arc<DecodingKey>>,
}

impl std::fmt::Debug for BearerTokenService {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerTokenService")
            .field("validation", &"[redacted]")
            .field("encoding", &"[redacted]")
            .field("decoding", &"[redacted]")
            .finish()
    }
}

impl From<BearerAuthConfig> for BearerTokenService {
    #[inline]
    fn from(value: BearerAuthConfig) -> Self {
        Self {
            validation: Arc::new(value.validation),
            encoding: value.encoding.map(Arc::new),
            decoding: value.decoding.map(Arc::new),
        }
    }
}

impl BearerTokenService {
    /// Returns validation rules  for JWT
    pub fn validation(&self) -> &Validation {
        &self.validation
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
        jsonwebtoken::encode(&JwtHeader::default(), claims, encoding_key)
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
        jsonwebtoken::decode(&*bearer.0, decoding_key, &self.validation)
            .map_err(Error::from)
            .map(|t| t.claims)
    }
}

/// Wraps a bearer token string
pub struct Bearer(Box<str>);

impl std::fmt::Debug for Bearer {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Bearer")
            .field(&"[redacted]")
            .finish()
    }
}

impl TryFrom<&HeaderValue> for Bearer {
    type Error = Error;
    
    #[inline]
    fn try_from(header: &HeaderValue) -> Result<Self, Self::Error> {
        let token = header
            .to_str()
            .map_err(Error::from)?;
        let token = token.strip_prefix(SCHEME)
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
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

impl TryFrom<&Extensions> for BearerTokenService {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        let bts = extensions
            .get::<BearerTokenService>()
            .ok_or_else(|| Error::server_error("Bearer Token authorization is not properly configured"))?;
        Ok(bts.clone())
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

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::time::{SystemTime, UNIX_EPOCH};
    use serde::{Serialize, Deserialize};
    use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey};
    use hyper::Request;
    use crate::headers::{HeaderMap, HeaderValue};
    use crate::http::Extensions;

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

        assert!(config.validation.algorithms.contains(&Algorithm::HS256));
        assert!(config.encoding.is_none());
        assert!(config.decoding.is_none());
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_set_decoding_key() {
        let decoding_key = DecodingKey::from_secret(SECRET);
        let config = BearerAuthConfig::default()
            .set_decoding_key(decoding_key);

        assert!(config.decoding.is_some());
        assert!(config.decoding_key().is_some());
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_set_encoding_key() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let config = BearerAuthConfig::default()
            .set_encoding_key(encoding_key);

        assert!(config.encoding.is_some());
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_with_alg() {
        let config = BearerAuthConfig::default()
            .with_alg(Algorithm::HS512)
            .with_alg(Algorithm::RS256);

        assert!(config.validation.algorithms.contains(&Algorithm::HS256));
        assert!(config.validation.algorithms.contains(&Algorithm::HS512));
        assert!(config.validation.algorithms.contains(&Algorithm::RS256));
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_with_aud() {
        let audience = vec!["test-audience", "another-audience"];
        let config = BearerAuthConfig::default()
            .with_aud(&audience);

        assert!(config.validation.aud.is_some());
        let expected_aud: HashSet<String> = audience.iter().map(|s| s.to_string()).collect();
        assert_eq!(config.validation.aud.unwrap(), expected_aud);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_with_iss() {
        let issuers = vec!["test-issuer", "another-issuer"];
        let config = BearerAuthConfig::default()
            .with_iss(&issuers);

        assert!(config.validation.iss.is_some());
        let expected_iss: HashSet<String> = issuers.iter().map(|s| s.to_string()).collect();
        assert_eq!(config.validation.iss.unwrap(), expected_iss);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_validate_aud() {
        let config = BearerAuthConfig::default()
            .validate_aud(false);

        assert!(!config.validation.validate_aud);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_validate_exp() {
        let config = BearerAuthConfig::default()
            .validate_exp(false);

        assert!(!config.validation.validate_exp);
    }

    #[tokio::test]
    async fn it_tests_bearer_auth_config_validate_nbf() {
        let config = BearerAuthConfig::default()
            .validate_nbf(true);

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
        assert!(service.validation().algorithms.contains(&Algorithm::HS256));
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_encode_success() {
        let encoding_key = EncodingKey::from_secret(SECRET);
        let config = BearerAuthConfig::default()
            .set_encoding_key(encoding_key);
        let service: BearerTokenService = config.into();

        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() + 3600;

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
        assert!(result.unwrap_err().to_string().contains("Missing security key"));
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
            .as_secs() + 3600;

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
        assert!(result.unwrap_err().to_string().contains("Missing security key"));
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
        assert!(result.unwrap_err().to_string().contains("Missing Credentials"));
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
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token_value}")).unwrap());

        let bearer = Bearer::try_from(&headers).unwrap();
        assert_eq!(bearer.to_string(), token_value);
    }

    #[tokio::test]
    async fn it_tests_bearer_from_header_map_missing_header() {
        let headers = HeaderMap::new();

        let result = Bearer::try_from(&headers);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing Authorization header"));
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
        let mut extensions = Extensions::new();
        let service = BearerTokenService::from(BearerAuthConfig::default());
        extensions.insert(service.clone());

        let result = BearerTokenService::try_from(&extensions);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn it_tests_bearer_token_service_from_extensions_missing() {
        let extensions = Extensions::new();

        let result = BearerTokenService::try_from(&extensions);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("Bearer Token authorization is not properly configured"));
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
    async fn it_tests_service_validation_getter() {
        let config = BearerAuthConfig::default()
            .validate_exp(false)
            .validate_aud(false);
        let service: BearerTokenService = config.into();

        let validation = service.validation();
        assert!(!validation.validate_exp);
        assert!(!validation.validate_aud);
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
            .as_secs() + 3600;

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

        assert_eq!(format!("{config:?}"), r#"BearerAuthConfig { validation: "[redacted]", encoding: "[redacted]", decoding: "[redacted]" }"#);
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

        assert_eq!(format!("{service:?}"), r#"BearerTokenService { validation: "[redacted]", encoding: "[redacted]", decoding: "[redacted]" }"#);
    }
}

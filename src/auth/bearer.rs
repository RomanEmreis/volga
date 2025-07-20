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
    pub fn with_iss<I, T>(mut self, aud: I) -> Self
    where
        T: ToString,
        I: AsRef<[T]>
    {
        self.validation.set_issuer(aud.as_ref());
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
    pub fn decode<C: DeserializeOwned>(&self, bearer: Bearer) -> Result<C, Error> {
        let Some(decoding_key) = &self.decoding else { 
            return Err(Error::server_error("Missing security key")); 
        };
        jsonwebtoken::decode(&bearer.0, decoding_key, &self.validation)
            .map_err(Error::from)
            .map(|t| t.claims)
    }
}

/// Wraps a bearer token string
pub struct Bearer(Box<str>);

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
    fn from_payload(payload: Payload) -> Self::Future {
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
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

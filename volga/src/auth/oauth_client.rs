//! OAuth 2.1 / OIDC issuer integration for bearer authentication
//!
//! Instead of configuring a static decoding key, an application can point
//! bearer authentication at an OAuth 2.1/OIDC issuer: the server metadata
//! (RFC 8414, with an OpenID Connect Discovery fallback) and the JSON Web
//! Key Set it advertises are fetched through
//! [`volga-oauth-client`](volga_oauth_client) and used to validate incoming
//! JWTs, keyed by the `kid` of each token.
//!
//! Enable it explicitly with [`App::use_oauth`](crate::App::use_oauth)
//! after describing the issuer with [`App::with_oauth`](crate::App::with_oauth):
//!
//! ```no_run
//! use volga::App;
//!
//! let mut app = App::new()
//!     .with_bearer_auth(|auth| auth.with_aud(["https://api.example.com"]))
//!     .with_oauth(|oauth| oauth.with_issuer("https://auth.example.com"));
//! app.use_oauth();
//! ```
//!
//! Keys are fetched lazily on the first request and refreshed when a token
//! arrives with an unknown `kid` (key rotation), rate-limited by
//! [`with_refresh_cooldown`](OAuthConfig::with_refresh_cooldown); concurrent
//! misses share a single refresh. While the issuer is unreachable and no
//! keys have been loaded yet, protected routes answer `503`.
//!
//! With the `config` feature the same knobs can be described in the
//! `[oauth.client]` section of the configuration file (fields present in
//! the file override builder calls; activation still requires
//! [`App::use_oauth`](crate::App::use_oauth) in code):
//!
//! ```toml
//! [oauth.client]
//! issuer = "https://auth.example.com"
//! refresh_cooldown_secs = 60   # optional
//! require_https = true         # optional
//! timeout_secs = 30            # optional
//! max_redirects = 5            # optional
//! ```

use jsonwebtoken::{
    Algorithm,
    jwk::{AlgorithmParameters, EllipticCurve, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse},
};
use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use volga_oauth_client::{ClientConfig, ClientError, DiscoveryClient};

use crate::error::Error;

/// Default minimum interval between two JWKS refresh attempts
pub const DEFAULT_REFRESH_COOLDOWN: Duration = Duration::from_secs(60);

/// Configuration of the OAuth 2.1/OIDC issuer whose keys validate bearer
/// tokens
///
/// Configured with [`App::with_oauth`](crate::App::with_oauth) and activated
/// explicitly with [`App::use_oauth`](crate::App::use_oauth). The issuer is
/// mandatory; everything else has production-safe defaults. Audience,
/// required claims and the other token checks stay on
/// [`BearerAuthConfig`](super::BearerAuthConfig).
pub struct OAuthConfig {
    pub(crate) issuer: Option<String>,
    client_config: ClientConfig,
    refresh_cooldown: Duration,
}

impl Default for OAuthConfig {
    #[inline]
    fn default() -> Self {
        Self {
            issuer: None,
            client_config: ClientConfig::new(),
            refresh_cooldown: DEFAULT_REFRESH_COOLDOWN,
        }
    }
}

impl Debug for OAuthConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthConfig")
            .field("issuer", &self.issuer)
            .field("client_config", &self.client_config)
            .field("refresh_cooldown", &self.refresh_cooldown)
            .finish()
    }
}

impl OAuthConfig {
    /// Sets the issuer identifier URL whose metadata and keys are used to
    /// validate bearer tokens; tokens must carry the same value in `iss`
    ///
    /// Discovery first tries the RFC 8414 path
    /// (`/.well-known/oauth-authorization-server`) and falls back to OpenID
    /// Connect Discovery (`/.well-known/openid-configuration`) when the
    /// former is not served.
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Adjusts the transport policy (HTTPS enforcement, timeout, redirect
    /// limit) used for discovery and JWKS requests
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// // a local development issuer served over plain HTTP
    /// let app = App::new().with_oauth(|oauth| oauth
    ///     .with_issuer("http://127.0.0.1:5000")
    ///     .with_client_config(|client| client.require_https(false)));
    /// ```
    pub fn with_client_config<F>(mut self, config: F) -> Self
    where
        F: FnOnce(ClientConfig) -> ClientConfig,
    {
        self.client_config = config(self.client_config);
        self
    }

    /// Sets the minimum interval between two JWKS refresh attempts
    /// (default: 60 seconds)
    ///
    /// A token with an unknown `kid` triggers a refresh — the cooldown
    /// keeps a flood of such tokens from hammering the issuer.
    pub fn with_refresh_cooldown(mut self, cooldown: Duration) -> Self {
        self.refresh_cooldown = cooldown;
        self
    }

    /// The configured minimum interval between two JWKS refresh attempts
    #[cfg(test)]
    pub(crate) fn refresh_cooldown(&self) -> Duration {
        self.refresh_cooldown
    }

    /// The transport policy used for discovery and JWKS requests
    #[cfg(test)]
    pub(crate) fn client_config(&self) -> &ClientConfig {
        &self.client_config
    }

    /// Builds the runtime key store; the caller has verified the issuer
    /// is present.
    pub(crate) fn into_store(self) -> JwksStore {
        JwksStore {
            issuer: self.issuer.expect("OAuth issuer is not configured"),
            client: DiscoveryClient::with_config(self.client_config),
            refresh_cooldown: self.refresh_cooldown,
            keys: RwLock::new(None),
            refresh: Mutex::new(None),
        }
    }
}

/// A verification key resolved from the issuer's JWKS
#[derive(Clone)]
pub(crate) struct KeyEntry {
    pub(crate) key: jsonwebtoken::DecodingKey,
    pub(crate) alg: Algorithm,
}

/// The current set of verification keys, indexed by `kid`
struct Keys {
    by_kid: HashMap<String, KeyEntry>,
    /// The only key of a single-key set, used when a token carries no `kid`
    single: Option<KeyEntry>,
}

impl Keys {
    fn from_set(set: &JwkSet) -> Self {
        let entries: Vec<(Option<String>, KeyEntry)> = set
            .keys
            .iter()
            .filter_map(|jwk| entry_from_jwk(jwk).map(|e| (jwk.common.key_id.clone(), e)))
            .collect();

        let single = match entries.as_slice() {
            [(_, entry)] => Some(entry.clone()),
            _ => None,
        };
        let by_kid = entries
            .into_iter()
            .filter_map(|(kid, entry)| kid.map(|kid| (kid, entry)))
            .collect();
        Self { by_kid, single }
    }

    fn lookup(&self, kid: Option<&str>) -> Option<KeyEntry> {
        match kid {
            Some(kid) => self
                .by_kid
                .get(kid)
                .cloned()
                // a single-key set without kids still serves tokens that
                // name a kid — there is nothing else the kid could select
                .or_else(|| {
                    self.by_kid
                        .is_empty()
                        .then(|| self.single.clone())
                        .flatten()
                }),
            None => self.single.clone(),
        }
    }
}

/// Converts a JWK into a verification key, skipping keys that are not
/// meant for (or usable in) signature verification.
fn entry_from_jwk(jwk: &Jwk) -> Option<KeyEntry> {
    if let Some(key_use) = &jwk.common.public_key_use
        && !matches!(key_use, PublicKeyUse::Signature)
    {
        return None;
    }
    // a symmetric key in a public JWKS is a token forgery kit: anyone who
    // reads the document holds the HMAC secret
    if matches!(jwk.algorithm, AlgorithmParameters::OctetKey(_)) {
        return None;
    }
    // an explicit non-signing or unknown `alg` disqualifies the key — the
    // inferred default is only for keys that don't declare one at all
    let alg = match jwk.common.key_algorithm {
        Some(alg) => signing_algorithm(alg)?,
        None => default_alg(&jwk.algorithm)?,
    };
    let key = jsonwebtoken::DecodingKey::from_jwk(jwk).ok()?;
    Some(KeyEntry { key, alg })
}

/// Maps a JWK `alg` to an asymmetric JWS signing algorithm; encryption
/// algorithms (`RSA-OAEP`, …) have no place in signature verification, and
/// symmetric ones (`HS*`) must never be driven by a public JWKS.
fn signing_algorithm(alg: KeyAlgorithm) -> Option<Algorithm> {
    match alg {
        KeyAlgorithm::ES256 => Some(Algorithm::ES256),
        KeyAlgorithm::ES384 => Some(Algorithm::ES384),
        KeyAlgorithm::RS256 => Some(Algorithm::RS256),
        KeyAlgorithm::RS384 => Some(Algorithm::RS384),
        KeyAlgorithm::RS512 => Some(Algorithm::RS512),
        KeyAlgorithm::PS256 => Some(Algorithm::PS256),
        KeyAlgorithm::PS384 => Some(Algorithm::PS384),
        KeyAlgorithm::PS512 => Some(Algorithm::PS512),
        KeyAlgorithm::EdDSA => Some(Algorithm::EdDSA),
        _ => None,
    }
}

/// Infers the signing algorithm for JWKs that omit `alg`, from the key
/// material itself; symmetric keys are never inferred — a public JWKS has
/// no business serving them.
fn default_alg(params: &AlgorithmParameters) -> Option<Algorithm> {
    match params {
        AlgorithmParameters::RSA(_) => Some(Algorithm::RS256),
        AlgorithmParameters::EllipticCurve(ec) => match ec.curve {
            EllipticCurve::P256 => Some(Algorithm::ES256),
            EllipticCurve::P384 => Some(Algorithm::ES384),
            _ => None,
        },
        AlgorithmParameters::OctetKeyPair(okp) => match okp.curve {
            EllipticCurve::Ed25519 => Some(Algorithm::EdDSA),
            _ => None,
        },
        AlgorithmParameters::OctetKey(_) => None,
    }
}

/// Lazily initialized, self-refreshing view of the issuer's JWKS
pub(crate) struct JwksStore {
    issuer: String,
    client: DiscoveryClient,
    refresh_cooldown: Duration,
    keys: RwLock<Option<Arc<Keys>>>,
    /// Time of the last refresh attempt; the lock also single-flights
    /// concurrent refreshes
    refresh: Mutex<Option<Instant>>,
}

impl Debug for JwksStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwksStore")
            .field("issuer", &self.issuer)
            .field("refresh_cooldown", &self.refresh_cooldown)
            .finish_non_exhaustive()
    }
}

impl JwksStore {
    /// The issuer identifier this store serves keys for
    #[cfg(test)]
    pub(crate) fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Resolves the verification key for a token's `kid`, refreshing the
    /// key set when the store is empty or the `kid` is unknown
    ///
    /// An unknown `kid` after a (possibly rate-limited) refresh is an
    /// invalid token (401-class error); a store that has never managed to
    /// load keys is a server-side problem (503-class error).
    pub(crate) async fn key_for(&self, kid: Option<&str>) -> Result<KeyEntry, Error> {
        if let Some(keys) = self.current()
            && let Some(entry) = keys.lookup(kid)
        {
            return Ok(entry);
        }

        self.refresh().await?;

        match self.current() {
            Some(keys) => keys.lookup(kid).ok_or_else(|| {
                // the freshest view the cooldown allows does not know this
                // kid — the token is signed with a key the issuer does not
                // (or no longer does) advertise
                Error::from_jwt_error(jsonwebtoken::errors::ErrorKind::InvalidToken.into())
            }),
            None => Err(Error::server_error(
                "OAuth issuer keys are not available yet",
            )),
        }
    }

    fn current(&self) -> Option<Arc<Keys>> {
        self.keys.read().expect("JWKS lock poisoned").clone()
    }

    /// Refetches metadata and JWKS unless a refresh ran within the
    /// cooldown; concurrent callers wait for the in-flight refresh and
    /// return without issuing their own
    async fn refresh(&self) -> Result<(), Error> {
        let mut last_attempt = self.refresh.lock().await;
        if let Some(at) = *last_attempt
            && at.elapsed() < self.refresh_cooldown
        {
            // within the cooldown the current view is authoritative
            return Ok(());
        }
        *last_attempt = Some(Instant::now());

        // re-discover metadata every time: a rotation may come along with
        // a jwks_uri move
        let metadata = match self.client.fetch_server_metadata(&self.issuer).await {
            Err(ClientError::Http(status)) if status.as_u16() == 404 => {
                self.client.fetch_oidc_metadata(&self.issuer).await
            }
            other => other,
        }
        .map_err(discovery_error)?;

        let document = self
            .client
            .fetch_jwks(&metadata)
            .await
            .map_err(discovery_error)?;
        let set: JwkSet = serde_json::from_value(document).map_err(|err| {
            Error::server_error(format!("OAuth issuer served an invalid JWKS: {err}"))
        })?;

        *self.keys.write().expect("JWKS lock poisoned") = Some(Arc::new(Keys::from_set(&set)));
        Ok(())
    }
}

/// Issuer communication failures are the resource server's infrastructure
/// problem (503-class), never the client's token.
fn discovery_error(err: ClientError) -> Error {
    Error::server_error(format!("OAuth issuer discovery failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rsa_jwk(kid: Option<&str>, alg: Option<&str>) -> serde_json::Value {
        // public components of the RSA test key used across the auth tests
        let mut jwk = json!({
            "kty": "RSA",
            "n": "q1ma_MoK5uWwsPxUNsVH1e-ybz_TzUGiFqUKbYkLTpXr9kpXi0i5SZOkGXHnLz1ch4gmOMuvvoLNwRyBzZGkOOd8IoLZAe4OAdmpQ2T0pY6szvUCK3WpIa06P7n20msOuc8bzm6CFM9fJU5_vHzeLGAj4Vi2GoFz4Lm3zUlZcY2zQWu2kdJZt6HbAM4s-nv1m3gqX-m5gTOjBP7oxEdNsOGZnl5v8h8uZ_U-CP2emvr67HW-Pph8OjVvXbyhBNGAbEljoXjJMLcqB5ULxXC4AspE-EfAZD5pCQO2ssUVPjw07qLNFd6gTJ7q41k2bNrS_SmYqWMeWttwEGS5Tjm3Xw",
            "e": "AQAB"
        });
        if let Some(kid) = kid {
            jwk["kid"] = kid.into();
        }
        if let Some(alg) = alg {
            jwk["alg"] = alg.into();
        }
        jwk
    }

    fn set_from(keys: Vec<serde_json::Value>) -> JwkSet {
        serde_json::from_value(json!({ "keys": keys })).unwrap()
    }

    #[test]
    fn it_defaults_and_builds_config() {
        let config = OAuthConfig::default();
        assert!(config.issuer.is_none());
        assert_eq!(config.refresh_cooldown, DEFAULT_REFRESH_COOLDOWN);

        let config = OAuthConfig::default()
            .with_issuer("https://auth.example.com")
            .with_client_config(|client| client.require_https(false))
            .with_refresh_cooldown(Duration::from_secs(5));
        assert_eq!(config.issuer.as_deref(), Some("https://auth.example.com"));
        assert_eq!(config.refresh_cooldown, Duration::from_secs(5));

        let store = config.into_store();
        assert_eq!(store.issuer(), "https://auth.example.com");
        assert!(format!("{store:?}").contains("auth.example.com"));
    }

    #[test]
    #[should_panic(expected = "OAuth issuer is not configured")]
    fn it_panics_building_a_store_without_issuer() {
        let _ = OAuthConfig::default().into_store();
    }

    #[test]
    fn it_indexes_keys_by_kid() {
        let keys = Keys::from_set(&set_from(vec![
            rsa_jwk(Some("a"), Some("RS256")),
            rsa_jwk(Some("b"), Some("RS384")),
        ]));

        assert_eq!(keys.lookup(Some("a")).unwrap().alg, Algorithm::RS256);
        assert_eq!(keys.lookup(Some("b")).unwrap().alg, Algorithm::RS384);
        assert!(keys.lookup(Some("c")).is_none());
        // several keys — a token without kid cannot pick one
        assert!(keys.lookup(None).is_none());
    }

    #[test]
    fn it_serves_a_single_key_set_without_kids() {
        let keys = Keys::from_set(&set_from(vec![rsa_jwk(None, Some("RS256"))]));
        // no kid in the token: the only key applies
        assert!(keys.lookup(None).is_some());
        // a kid in the token: nothing else the kid could have selected
        assert!(keys.lookup(Some("whatever")).is_some());
    }

    #[test]
    fn it_infers_algorithms_for_keys_without_alg() {
        let keys = Keys::from_set(&set_from(vec![rsa_jwk(Some("a"), None)]));
        assert_eq!(keys.lookup(Some("a")).unwrap().alg, Algorithm::RS256);
    }

    #[test]
    fn it_skips_unusable_keys() {
        let mut encryption_key = rsa_jwk(Some("enc"), Some("RS256"));
        encryption_key["use"] = "enc".into();
        // symmetric keys are never accepted from a public JWKS — with or
        // without an explicit `alg`, the published `k` would hand every
        // reader the HMAC signing secret
        let symmetric = json!({ "kty": "oct", "kid": "sym", "k": "c2VjcmV0" });
        let symmetric_hs256 =
            json!({ "kty": "oct", "kid": "sym-hs", "k": "c2VjcmV0", "alg": "HS256" });
        // an explicit non-signing alg must not fall back to the inferred
        // default — the issuer did not advertise this key for signing
        let oaep = rsa_jwk(Some("oaep"), Some("RSA-OAEP"));

        let keys = Keys::from_set(&set_from(vec![
            encryption_key,
            symmetric,
            symmetric_hs256,
            oaep,
        ]));
        assert!(keys.lookup(Some("enc")).is_none());
        assert!(keys.lookup(Some("sym")).is_none());
        assert!(keys.lookup(Some("sym-hs")).is_none());
        assert!(keys.lookup(Some("oaep")).is_none());
        assert!(keys.lookup(None).is_none());
    }
}

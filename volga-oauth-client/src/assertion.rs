//! Client assertions for `private_key_jwt` (RFC 7523 Section 2.2)
//!
//! [`PrivateKeyJwt`] holds the asymmetric key a confidential client
//! authenticates with: instead of sending a shared secret, every token
//! request carries a short-lived JWS the client signed itself. Attach it
//! with [`OAuthClient::with_private_key_jwt`](crate::OAuthClient::with_private_key_jwt)
//! and it applies to every grant the client sends.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use volga_oauth_core::{AuthorizationServerMetadata, JwkSet, PublicJwk, pem::PemKind};

use crate::{ClientError, JwsAlgorithm, pkce::random_urlsafe};

/// Default validity of a generated client assertion
///
/// RFC 7523 Section 3 requires an `exp` and expects it to be short - the
/// assertion is consumed by a single token request.
pub const DEFAULT_ASSERTION_LIFETIME: Duration = Duration::from_secs(60);

/// The key families a [`JwsAlgorithm`] can be keyed by, selecting the
/// constructor a PEM or DER blob is loaded with.
#[derive(Clone, Copy)]
enum KeyFamily {
    Rsa,
    Ec,
    Ed,
}

impl KeyFamily {
    /// Returns `false` when a PEM header rules this family out.
    ///
    /// [`PemKind::Ambiguous`] is the PKCS#8 / SPKI header, which carries
    /// any family (and is the only form an Ed25519 key comes in), and
    /// [`PemKind::Unknown`] is no evidence either way - only a header that
    /// positively names the *other* family is a contradiction. Judging it
    /// here turns an opaque `InvalidKeyFormat` from the parser into an
    /// error that says which of the two the caller got wrong.
    fn accepts(self, header: PemKind) -> bool {
        match self {
            Self::Rsa => header != PemKind::Ec,
            Self::Ec => header != PemKind::Rsa,
            Self::Ed => !matches!(header, PemKind::Rsa | PemKind::Ec),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Rsa => "an RSA",
            Self::Ec => "an EC",
            Self::Ed => "an Ed25519",
        }
    }
}

/// Maps an algorithm onto the underlying JWT implementation and the key
/// family it needs.
///
/// The symmetric algorithms have no place here: `private_key_jwt` proves
/// possession of a key the authorization server does not hold, and an
/// HMAC secret proves nothing of the sort.
fn asymmetric(algorithm: JwsAlgorithm) -> Result<(Algorithm, KeyFamily), ClientError> {
    Ok(match algorithm {
        JwsAlgorithm::RS256 => (Algorithm::RS256, KeyFamily::Rsa),
        JwsAlgorithm::RS384 => (Algorithm::RS384, KeyFamily::Rsa),
        JwsAlgorithm::RS512 => (Algorithm::RS512, KeyFamily::Rsa),
        JwsAlgorithm::PS256 => (Algorithm::PS256, KeyFamily::Rsa),
        JwsAlgorithm::PS384 => (Algorithm::PS384, KeyFamily::Rsa),
        JwsAlgorithm::PS512 => (Algorithm::PS512, KeyFamily::Rsa),
        JwsAlgorithm::ES256 => (Algorithm::ES256, KeyFamily::Ec),
        JwsAlgorithm::ES384 => (Algorithm::ES384, KeyFamily::Ec),
        JwsAlgorithm::EdDSA => (Algorithm::EdDSA, KeyFamily::Ed),
        symmetric => {
            return Err(ClientError::signing(format!(
                "{symmetric} is a shared-secret algorithm; private_key_jwt requires an \
                 asymmetric key the authorization server does not hold"
            )));
        }
    })
}

/// The signing key and claims policy of `private_key_jwt` client
/// authentication (RFC 7523 Section 2.2)
///
/// The generated assertion carries `iss` = `sub` = the client identifier,
/// `aud` identifying the authorization server, a random `jti` and an
/// expiry [`DEFAULT_ASSERTION_LIFETIME`] ahead - a fresh one per token
/// request, so a captured assertion is not replayable for long.
///
/// # Example
/// ```no_run
/// use volga_oauth_client::{JwsAlgorithm, OAuthClient, PrivateKeyJwt};
///
/// # fn run(pem: &[u8]) -> Result<(), volga_oauth_client::ClientError> {
/// let key = PrivateKeyJwt::from_pem(pem, JwsAlgorithm::RS256)?
///     .with_key_id("2026-08");
///
/// let client = OAuthClient::new("my-client").with_private_key_jwt(key);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct PrivateKeyJwt {
    key: Arc<EncodingKey>,
    algorithm: JwsAlgorithm,
    /// The `alg` header value, resolved once at construction
    alg: Algorithm,
    key_id: Option<String>,
    lifetime: Duration,
    audiences: Vec<String>,
    public_jwk: Option<PublicJwk>,
}

impl PrivateKeyJwt {
    /// Loads a PEM-encoded private key for `algorithm`
    ///
    /// RSA keys are expected in PKCS#1 or PKCS#8 form, EC and Ed25519 keys
    /// in PKCS#8 form. Fails with [`ClientError::Signing`] when `algorithm`
    /// is symmetric (an HMAC secret cannot back this method), or when the
    /// key does not parse or does not match the algorithm family.
    pub fn from_pem(pem: &[u8], algorithm: JwsAlgorithm) -> Result<Self, ClientError> {
        let (alg, family) = asymmetric(algorithm)?;

        // the header names the family, never the algorithm: it can only
        // rule a key out, and does so with a better message than the
        // parser's `InvalidKeyFormat`
        let header = volga_oauth_core::pem::detect(pem);
        if !family.accepts(header) {
            return Err(ClientError::signing(format!(
                "{algorithm} needs {} key, but the PEM header describes {header:?}",
                family.name()
            )));
        }

        let key = match family {
            KeyFamily::Rsa => EncodingKey::from_rsa_pem(pem),
            KeyFamily::Ec => EncodingKey::from_ec_pem(pem),
            KeyFamily::Ed => EncodingKey::from_ed_pem(pem),
        }
        .map_err(ClientError::signing)?;

        Ok(Self::from_key(key, algorithm, alg))
    }

    /// Loads the PEM file at `path` as the signing key for `algorithm`
    ///
    /// A convenience over [`from_pem`](Self::from_pem) for the common case
    /// of a key mounted as a file; the same failures apply, plus an I/O
    /// error when the file cannot be read.
    ///
    /// # Example
    /// ```no_run
    /// use volga_oauth_client::{JwsAlgorithm, OAuthClient, PrivateKeyJwt};
    ///
    /// # fn run() -> Result<(), volga_oauth_client::ClientError> {
    /// let key = PrivateKeyJwt::from_pem_file("/etc/secrets/client.pem", JwsAlgorithm::RS256)?;
    /// let client = OAuthClient::new("my-client").with_private_key_jwt(key);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_pem_file(
        path: impl AsRef<std::path::Path>,
        algorithm: JwsAlgorithm,
    ) -> Result<Self, ClientError> {
        let path = path.as_ref();
        let pem = std::fs::read(path).map_err(|err| {
            ClientError::signing(format!(
                "failed to read the signing key at {}: {err}",
                path.display()
            ))
        })?;

        Self::from_pem(&pem, algorithm)
    }

    /// Adopts a DER-encoded private key for `algorithm`
    ///
    /// Unlike [`from_pem`](Self::from_pem) the bytes are taken as-is; a
    /// key that does not match the algorithm surfaces at the first signing
    /// attempt rather than here.
    pub fn from_der(der: &[u8], algorithm: JwsAlgorithm) -> Result<Self, ClientError> {
        let (alg, family) = asymmetric(algorithm)?;
        let key = match family {
            KeyFamily::Rsa => EncodingKey::from_rsa_der(der),
            KeyFamily::Ec => EncodingKey::from_ec_der(der),
            KeyFamily::Ed => EncodingKey::from_ed_der(der),
        };

        Ok(Self::from_key(key, algorithm, alg))
    }

    fn from_key(key: EncodingKey, algorithm: JwsAlgorithm, alg: Algorithm) -> Self {
        Self {
            key: Arc::new(key),
            algorithm,
            alg,
            key_id: None,
            lifetime: DEFAULT_ASSERTION_LIFETIME,
            audiences: Vec::new(),
            public_jwk: None,
        }
    }

    /// Attaches the public half of this key, so it can be published for the
    /// authorization server to verify assertions with
    ///
    /// Supply the *public* key only - this crate signs, it does not derive
    /// public keys from private ones. [`PublicJwk`] holds public material
    /// exclusively, so there is no way to publish the signing key by
    /// accident through it.
    ///
    /// [`jwks`](Self::jwks) then renders the JWK Set to serve, whether as
    /// the `jwks` member of a Dynamic Client Registration request, or as
    /// the document a `jwks_uri` points at.
    ///
    /// A `kid` on `jwk` is adopted when this key has none of its own, so
    /// the assertions start naming the key the document publishes. An
    /// explicit [`with_key_id`](Self::with_key_id) wins in either order.
    ///
    /// Fails with [`ClientError::Signing`] when the material cannot carry
    /// the algorithm the assertions are signed with - publishing an RSA
    /// key for an `ES256` signature, or a P-384 key for `ES256`, yields a
    /// document the authorization server cannot verify anything against.
    /// This is the half [`PublicJwk`] cannot check on its own: only this
    /// key knows what the assertions are actually signed with.
    pub fn with_public_jwk(mut self, jwk: PublicJwk) -> Result<Self, ClientError> {
        if !jwk.key().supports(self.algorithm) {
            return Err(ClientError::signing(format!(
                "the public JWK is {} key material, which cannot carry the {} the \
                 assertions are signed with",
                jwk.key().key_type(),
                self.algorithm
            )));
        }

        // otherwise the published document would name a `kid` that no
        // assertion carries, and a server selecting the client's key by it
        // would find nothing
        if self.key_id.is_none() {
            self.key_id = jwk.key_id().map(ToOwned::to_owned);
        }

        self.public_jwk = Some(jwk);
        Ok(self)
    }

    /// Returns the JWK Set to publish for this key, or `None` when no
    /// public JWK was attached with
    /// [`with_public_jwk`](Self::with_public_jwk)
    ///
    /// `kid` and `alg` are taken from this key's configuration so the
    /// published document agrees with what the assertions actually carry -
    /// a `kid` mismatch is what makes a server unable to find the key it
    /// should verify with. The document therefore names a `kid` exactly
    /// when the assertions do.
    ///
    /// # Example
    /// ```
    /// # use volga_oauth_client::{ClientError, JwsAlgorithm, PrivateKeyJwt, PublicJwk};
    /// # fn run(pem: &[u8], public_jwk: PublicJwk) -> Result<(), ClientError> {
    /// let key = PrivateKeyJwt::from_pem(pem, JwsAlgorithm::ES256)?
    ///     .with_key_id("2026-08")
    ///     .with_public_jwk(public_jwk)?;
    ///
    /// let published = &key.jwks().unwrap().keys[0];
    /// assert_eq!(published.key_id(), Some("2026-08"));
    /// assert_eq!(published.algorithm(), Some(JwsAlgorithm::ES256));
    /// # Ok(())
    /// # }
    /// ```
    pub fn jwks(&self) -> Option<JwkSet> {
        // `with_public_jwk` refused any material that cannot carry this
        // algorithm, so stamping it on cannot contradict the key
        let mut jwk = self
            .public_jwk
            .clone()?
            .with_algorithm(self.algorithm)
            .expect("the attached JWK was checked against this algorithm");
        if let Some(key_id) = &self.key_id {
            jwk = jwk.with_key_id(key_id);
        }

        Some(JwkSet::new([jwk]))
    }

    /// Sets the `kid` header identifying this key among the ones published
    /// in the client's JWK Set
    ///
    /// Servers that hold more than one key for a client need it to pick
    /// the right one; it is otherwise optional.
    pub fn with_key_id(mut self, key_id: impl Into<String>) -> Self {
        self.key_id = Some(key_id.into());
        self
    }

    /// Overrides how long a generated assertion stays valid
    /// ([`DEFAULT_ASSERTION_LIFETIME`] by default)
    pub fn with_lifetime(mut self, lifetime: Duration) -> Self {
        self.lifetime = lifetime;
        self
    }

    /// Overrides the `aud` claim, which defaults to the issuer identifier
    /// from the server metadata
    ///
    /// RFC 7523 Section 3 also permits the token endpoint URL, which some
    /// servers require instead. Passing several values produces a JSON
    /// array - useful when a server is inconsistent about the trailing
    /// slash of its issuer, since `aud` matching is membership-based
    /// (RFC 7519 Section 4.1.3).
    pub fn with_audiences<I, S>(mut self, audiences: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.audiences = audiences.into_iter().map(Into::into).collect();
        self
    }

    /// Returns the algorithm assertions are signed with
    #[inline]
    pub fn algorithm(&self) -> JwsAlgorithm {
        self.algorithm
    }

    /// Returns the `kid` header set with [`with_key_id`](Self::with_key_id)
    #[inline]
    pub fn key_id(&self) -> Option<&str> {
        self.key_id.as_deref()
    }

    /// Signs a client assertion for `client_id` against `metadata`.
    ///
    /// Rejects the attempt upfront when the server advertises the JWS
    /// algorithms it accepts for client authentication and this one is not
    /// among them - the token endpoint would answer `invalid_client`.
    pub(crate) fn assertion(
        &self,
        client_id: &str,
        metadata: &AuthorizationServerMetadata,
    ) -> Result<String, ClientError> {
        let advertised = &metadata.token_endpoint_auth_signing_alg_values_supported;
        if !advertised.is_empty() && !advertised.iter().any(|alg| alg == self.algorithm.as_str()) {
            return Err(ClientError::validation(format!(
                "authorization server does not accept {} for client authentication; \
                 it advertises {advertised:?}",
                self.algorithm
            )));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ClientError::validation("system clock is set before the Unix epoch"))?
            .as_secs();

        let claims = AssertionClaims {
            iss: client_id,
            sub: client_id,
            aud: match self.audiences.as_slice() {
                [] => Audience::One(&metadata.issuer),
                [single] => Audience::One(single),
                many => Audience::Many(many),
            },
            jti: random_urlsafe(16),
            iat: now,
            // a lifetime too large to represent lands at the far end of
            // the epoch instead of overflowing
            exp: now.saturating_add(self.lifetime.as_secs()),
        };

        let mut header = Header::new(self.alg);
        header.kid.clone_from(&self.key_id);

        encode(&header, &claims, &self.key).map_err(ClientError::signing)
    }
}

/// Two [`PrivateKeyJwt`] values are equal when they were built from the
/// same key handle (`Arc` identity - key material is never compared) and
/// carry the same claims policy.
impl PartialEq for PrivateKeyJwt {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.key, &other.key)
            && self.algorithm == other.algorithm
            && self.key_id == other.key_id
            && self.lifetime == other.lifetime
            && self.audiences == other.audiences
            && self.public_jwk == other.public_jwk
    }
}

impl Eq for PrivateKeyJwt {}

impl std::fmt::Debug for PrivateKeyJwt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // the key is a credential - never expose it in debug output
        f.debug_struct("PrivateKeyJwt")
            .field("key", &"[redacted]")
            .field("algorithm", &self.algorithm)
            .field("key_id", &self.key_id)
            .field("lifetime", &self.lifetime)
            .field("audiences", &self.audiences)
            // the public half is not a secret - it is meant to be published
            .field("public_jwk", &self.public_jwk)
            .finish()
    }
}

/// The claims of a `private_key_jwt` assertion (RFC 7523 Section 3).
#[derive(Serialize)]
struct AssertionClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: Audience<'a>,
    jti: String,
    iat: u64,
    exp: u64,
}

/// `aud` is a single string or an array of them (RFC 7519 Section 4.1.3).
#[derive(Serialize)]
#[serde(untagged)]
enum Audience<'a> {
    One(&'a str),
    Many(&'a [String]),
}

/// A throwaway P-256 key in PKCS#8 PEM form, shared by the crate's tests.
#[cfg(test)]
pub(crate) const TEST_EC_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgIeRig+AlqV2rBdgt
BzEQ28UAk8/d5l2+4PDfsspynmShRANCAATP07xL4i2PpomWJmZSZZMbQqj4Ybbd
aLozept2OHnD6J7pNTHm12NdaEJ4knzrCkp6pho2EFIQh5cKnqHm+hQw
-----END PRIVATE KEY-----";

/// Loads [`TEST_EC_PEM`] as a signing key.
#[cfg(test)]
pub(crate) fn test_key() -> PrivateKeyJwt {
    PrivateKeyJwt::from_pem(TEST_EC_PEM, JwsAlgorithm::ES256).expect("the test key must parse")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::Value;

    const EC_PEM: &[u8] = TEST_EC_PEM;

    fn metadata() -> AuthorizationServerMetadata {
        let mut metadata = AuthorizationServerMetadata::new("https://auth.example.com");
        metadata.token_endpoint = Some("https://auth.example.com/token".into());
        metadata
    }

    fn key() -> PrivateKeyJwt {
        test_key()
    }

    /// Decodes the header and claims of an unverified JWS.
    fn parts(token: &str) -> (Value, Value) {
        let decode = |part: &str| {
            serde_json::from_slice::<Value>(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
        };
        let mut parts = token.split('.');
        let header = decode(parts.next().unwrap());
        let claims = decode(parts.next().unwrap());
        assert!(parts.next().is_some(), "the signature part is missing");
        (header, claims)
    }

    #[test]
    fn it_signs_an_rfc7523_assertion() {
        let assertion = key().assertion("my-client", &metadata()).unwrap();
        let (header, claims) = parts(&assertion);

        assert_eq!(header["alg"], "ES256");
        assert_eq!(header["typ"], "JWT");
        assert!(header.get("kid").is_none());

        assert_eq!(claims["iss"], "my-client");
        assert_eq!(claims["sub"], "my-client");
        assert_eq!(claims["aud"], "https://auth.example.com");
        assert!(claims["jti"].as_str().is_some_and(|jti| !jti.is_empty()));

        let (iat, exp) = (
            claims["iat"].as_u64().unwrap(),
            claims["exp"].as_u64().unwrap(),
        );
        assert_eq!(exp - iat, DEFAULT_ASSERTION_LIFETIME.as_secs());
    }

    #[test]
    fn it_mints_a_fresh_jti_per_assertion() {
        let key = key();
        let first = key.assertion("my-client", &metadata()).unwrap();
        let second = key.assertion("my-client", &metadata()).unwrap();
        assert_ne!(parts(&first).1["jti"], parts(&second).1["jti"]);
    }

    #[test]
    fn it_applies_the_configured_claims_policy() {
        let key = key()
            .with_key_id("k-1")
            .with_lifetime(Duration::from_secs(5))
            .with_audiences(["https://auth.example.com/token"]);
        assert_eq!(key.key_id(), Some("k-1"));
        assert_eq!(key.algorithm(), JwsAlgorithm::ES256);

        let (header, claims) = parts(&key.assertion("my-client", &metadata()).unwrap());
        assert_eq!(header["kid"], "k-1");
        assert_eq!(claims["aud"], "https://auth.example.com/token");
        assert_eq!(
            claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
            5
        );

        // several audiences become an array, so a server matching either
        // spelling of its issuer still finds itself in there
        let key = key.with_audiences(["https://auth.example.com", "https://auth.example.com/"]);
        let (_, claims) = parts(&key.assertion("my-client", &metadata()).unwrap());
        assert_eq!(
            claims["aud"],
            serde_json::json!(["https://auth.example.com", "https://auth.example.com/"])
        );
    }

    #[test]
    fn it_rejects_an_algorithm_the_server_does_not_accept() {
        let mut metadata = metadata();
        metadata.token_endpoint_auth_signing_alg_values_supported = vec!["RS256".into()];

        let err = key().assertion("my-client", &metadata).unwrap_err();
        assert!(matches!(err, ClientError::Validation(reason) if reason.contains("ES256")));

        // an advertised match goes through, and so does no advertisement
        metadata.token_endpoint_auth_signing_alg_values_supported =
            vec!["RS256".into(), "ES256".into()];
        assert!(key().assertion("my-client", &metadata).is_ok());
    }

    #[test]
    fn it_rejects_a_key_that_does_not_parse() {
        assert!(matches!(
            PrivateKeyJwt::from_pem(b"not a pem", JwsAlgorithm::RS256),
            Err(ClientError::Signing(_))
        ));
        // ...including one from the wrong family. The header here is the
        // unqualified PKCS#8 one, so nothing rules it out up front - the
        // key parser is what refuses it
        assert!(matches!(
            PrivateKeyJwt::from_pem(EC_PEM, JwsAlgorithm::RS256),
            Err(ClientError::Signing(_))
        ));
    }

    #[test]
    fn it_names_the_family_a_pem_header_rules_out() {
        // a header that positively names the other family is caught before
        // the parser, so the message says which half is wrong
        let sec1_ec = b"-----BEGIN EC PRIVATE KEY-----\nabc\n-----END EC PRIVATE KEY-----";
        let err = PrivateKeyJwt::from_pem(sec1_ec, JwsAlgorithm::RS256).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("RS256") && message.contains("RSA"),
            "{message}"
        );
        assert!(message.contains("Ec"), "{message}");

        let pkcs1_rsa = b"-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----";
        for alg in [JwsAlgorithm::ES256, JwsAlgorithm::EdDSA] {
            let message = PrivateKeyJwt::from_pem(pkcs1_rsa, alg)
                .unwrap_err()
                .to_string();
            assert!(message.contains("Rsa"), "{message}");
        }

        // ...while the PKCS#8 header carries any family and is never the
        // grounds for rejection
        assert!(PrivateKeyJwt::from_pem(EC_PEM, JwsAlgorithm::ES256).is_ok());
    }

    #[test]
    fn it_loads_a_key_from_a_pem_file() {
        let path = std::env::temp_dir().join(format!(
            "volga-test-private-key-jwt-{}.pem",
            std::process::id()
        ));
        std::fs::write(&path, EC_PEM).unwrap();
        let key = PrivateKeyJwt::from_pem_file(&path, JwsAlgorithm::ES256);
        let _ = std::fs::remove_file(&path);
        assert!(key.is_ok(), "got: {key:?}");

        let err =
            PrivateKeyJwt::from_pem_file("/nonexistent/volga/client.pem", JwsAlgorithm::ES256)
                .unwrap_err();
        assert!(
            matches!(&err, ClientError::Signing(_)) && err.to_string().contains("client.pem"),
            "the error must name the file it could not read, got: {err}"
        );
    }

    #[test]
    fn it_rejects_symmetric_algorithms() {
        // the shared vocabulary of `JwsAlgorithm` includes the HMAC
        // algorithms, but a secret the server already holds proves nothing
        // about who signed - this method has to refuse them
        for alg in [
            JwsAlgorithm::HS256,
            JwsAlgorithm::HS384,
            JwsAlgorithm::HS512,
        ] {
            let err = PrivateKeyJwt::from_pem(EC_PEM, alg).unwrap_err();
            assert!(
                matches!(&err, ClientError::Signing(_)) && err.to_string().contains("asymmetric"),
                "got: {err}"
            );
            assert!(PrivateKeyJwt::from_der(b"whatever", alg).is_err());
        }

        // ...while an asymmetric one loaded from DER is taken as-is
        assert!(PrivateKeyJwt::from_der(b"not really a key", JwsAlgorithm::RS256).is_ok());
    }

    #[test]
    fn it_compares_by_key_handle_and_policy() {
        let key = key();
        assert_eq!(key, key.clone());
        assert_ne!(key, key.clone().with_key_id("k-1"));
        // same material, different handle
        assert_ne!(key, self::key());
    }

    /// The public half of [`TEST_EC_PEM`].
    fn public_jwk() -> PublicJwk {
        PublicJwk::new(volga_oauth_core::jwk::PublicKey::Ec {
            crv: volga_oauth_core::jwk::EcCurve::P256,
            x: "z9O8S-Itj6aJliZmUmWTG0Ko-GG23Wi6M3qbdjh5w-g".into(),
            y: "nuk1MebXY11oQniSfOsKSnqmGjYQUhCHlwqeoeb6FDA".into(),
        })
    }

    #[test]
    fn it_publishes_a_jwk_set_agreeing_with_the_assertions() {
        // no public JWK attached - nothing to publish
        assert!(key().jwks().is_none());

        let key = key()
            .with_key_id("2026-08")
            .with_public_jwk(public_jwk())
            .unwrap();
        let jwks = key.jwks().unwrap();
        let published = &jwks.keys[0];

        // the `kid` and `alg` must match what the assertion header carries,
        // or the server cannot pick the key to verify with
        let assertion = key.assertion("my-client", &metadata()).unwrap();
        let (header, _) = parts(&assertion);
        assert_eq!(published.key_id(), Some("2026-08"));
        assert_eq!(published.key_id(), header["kid"].as_str());
        assert_eq!(published.algorithm(), Some(JwsAlgorithm::ES256));
        assert_eq!(published.algorithm().unwrap().as_str(), header["alg"]);

        // ...and the published key must actually verify the signature -
        // the whole point of publishing it
        let document = serde_json::to_value(published).unwrap();
        let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_value(document).unwrap();
        let decoding = jsonwebtoken::DecodingKey::from_jwk(&jwk).unwrap();
        let mut validation = jsonwebtoken::Validation::new(Algorithm::ES256);
        validation.set_issuer(&["my-client"]);
        validation.set_audience(&["https://auth.example.com"]);
        jsonwebtoken::decode::<Value>(&assertion, &decoding, &validation)
            .expect("the published JWK must verify the assertion it was published for");

        // an `alg` left unset on the JWK is filled in from the signing
        // configuration: the assertions are what the document describes
        let jwks = key.with_public_jwk(public_jwk()).unwrap().jwks().unwrap();
        assert_eq!(jwks.keys[0].algorithm(), Some(JwsAlgorithm::ES256));
    }

    #[test]
    fn it_refuses_a_public_jwk_that_cannot_carry_the_signature() {
        // an RSA document published for an ES256 signature leaves the
        // server with nothing it can verify against
        let rsa = PublicJwk::new(volga_oauth_core::jwk::PublicKey::Rsa {
            n: "n".into(),
            e: "AQAB".into(),
        });
        let err = key().with_public_jwk(rsa).unwrap_err();
        assert!(
            matches!(&err, ClientError::Signing(_))
                && err.to_string().contains("RSA")
                && err.to_string().contains("ES256"),
            "got: {err}"
        );

        // ...and so does the right key type on the wrong curve
        let p384 = PublicJwk::new(volga_oauth_core::jwk::PublicKey::Ec {
            crv: volga_oauth_core::jwk::EcCurve::P384,
            x: "x".into(),
            y: "y".into(),
        });
        assert!(key().with_public_jwk(p384).is_err());
    }

    #[test]
    fn it_publishes_a_kid_exactly_when_the_assertions_carry_one() {
        let kid_of = |key: &PrivateKeyJwt| {
            let published = key.jwks().unwrap().keys[0].key_id().map(str::to_owned);
            let signed = parts(&key.assertion("my-client", &metadata()).unwrap()).0["kid"]
                .as_str()
                .map(str::to_owned);
            assert_eq!(
                published, signed,
                "the published document and the assertion must name the same key"
            );
            published
        };

        // neither names one
        assert_eq!(kid_of(&key().with_public_jwk(public_jwk()).unwrap()), None);

        // a `kid` on the JWK alone is adopted, so the assertions start
        // naming the key the document publishes
        let adopted = key()
            .with_public_jwk(public_jwk().with_key_id("from-the-jwk"))
            .unwrap();
        assert_eq!(kid_of(&adopted), Some("from-the-jwk".into()));

        // an explicit one wins, whichever order the two are set in
        let jwk_first = key()
            .with_public_jwk(public_jwk().with_key_id("from-the-jwk"))
            .unwrap()
            .with_key_id("explicit");
        assert_eq!(kid_of(&jwk_first), Some("explicit".into()));

        let id_first = key()
            .with_key_id("explicit")
            .with_public_jwk(public_jwk().with_key_id("from-the-jwk"))
            .unwrap();
        assert_eq!(kid_of(&id_first), Some("explicit".into()));
    }

    #[test]
    fn it_redacts_the_key_in_debug_output() {
        let debug = format!("{:?}", key());
        assert!(debug.contains("[redacted]"));
        assert!(debug.contains("ES256"));
    }

    #[test]
    fn it_survives_an_unrepresentable_lifetime() {
        // a lifetime that would overflow the epoch must not panic
        let key = key().with_lifetime(Duration::MAX);
        let (_, claims) = parts(&key.assertion("my-client", &metadata()).unwrap());
        assert_eq!(claims["exp"].as_u64(), Some(u64::MAX));
    }
}

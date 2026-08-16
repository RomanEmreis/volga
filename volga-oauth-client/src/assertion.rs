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
use volga_oauth_core::AuthorizationServerMetadata;

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
        let key = match family {
            KeyFamily::Rsa => EncodingKey::from_rsa_pem(pem),
            KeyFamily::Ec => EncodingKey::from_ec_pem(pem),
            KeyFamily::Ed => EncodingKey::from_ed_pem(pem),
        }
        .map_err(ClientError::signing)?;

        Ok(Self::from_key(key, algorithm, alg))
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
        }
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
        // ...including one from the wrong family
        assert!(matches!(
            PrivateKeyJwt::from_pem(EC_PEM, JwsAlgorithm::RS256),
            Err(ClientError::Signing(_))
        ));
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

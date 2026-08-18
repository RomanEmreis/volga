//! Public JSON Web Keys ([RFC 7517](https://www.rfc-editor.org/rfc/rfc7517))
//!
//! Only what a client or server publishes for *signature verification*:
//! [`PublicJwk`] holds the public members of one key and nothing else, so a
//! private key cannot be published through it by accident, and [`JwkSet`]
//! is the document those keys are served in - the `jwks` member of a
//! registration request, or whatever a `jwks_uri` points at.

use serde::{Deserialize, Serialize, de::Error as _};

use crate::algorithm::JwsAlgorithm;

/// A JSON Web Key Set (RFC 7517 Section 5) - the document a `jwks_uri` serves
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwkSet {
    /// The keys in the set
    pub keys: Vec<PublicJwk>,
}

impl JwkSet {
    /// Creates a set holding the given keys
    pub fn new<I: IntoIterator<Item = PublicJwk>>(keys: I) -> Self {
        Self {
            keys: keys.into_iter().collect(),
        }
    }

    /// Returns the key with the given `kid`, if the set holds one
    pub fn find(&self, key_id: &str) -> Option<&PublicJwk> {
        self.keys.iter().find(|key| key.key_id() == Some(key_id))
    }
}

/// The declared use of a published key
///
/// Signature verification is the only one this crate deals with, so a
/// document declaring `"use": "enc"` is refused rather than silently
/// treated as a signing key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyUse {
    /// `sig` - the key verifies signatures (RFC 7517 Section 4.2)
    #[default]
    #[serde(rename = "sig")]
    Signature,
}

/// A curve an elliptic-curve (`EC`) key can be on
///
/// The set is closed to the curves [`JwsAlgorithm`] can sign with. It is
/// separate from [`OkpCurve`] on purpose: `"kty": "EC"` with an Edwards
/// curve is not a key any verifier can use, and a shared curve type would
/// let that document be built and published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EcCurve {
    /// NIST P-256, paired with `ES256`
    #[serde(rename = "P-256")]
    P256,
    /// NIST P-384, paired with `ES384`
    #[serde(rename = "P-384")]
    P384,
}

/// A curve an octet key pair (`OKP`) can be on
///
/// Only the signing curve of RFC 8037: `X25519` and `X448` are
/// key-agreement curves and never carry a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OkpCurve {
    /// Ed25519, paired with `EdDSA`
    Ed25519,
}

/// The key material of a [`PublicJwk`], by key type (RFC 7518 Section 6)
///
/// Every member is the base64url encoding the JWK carries it in; this
/// crate does no cryptography and never interprets them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kty")]
#[non_exhaustive]
pub enum PublicKey {
    /// An elliptic curve key (RFC 7518 Section 6.2.1)
    #[serde(rename = "EC")]
    Ec {
        /// The curve the key is on
        crv: EcCurve,
        /// The x coordinate
        x: String,
        /// The y coordinate
        y: String,
    },

    /// An RSA key (RFC 7518 Section 6.3.1)
    #[serde(rename = "RSA")]
    Rsa {
        /// The modulus
        n: String,
        /// The public exponent
        e: String,
    },

    /// An octet key pair - the Edwards-curve key type (RFC 8037 Section 2)
    #[serde(rename = "OKP")]
    Okp {
        /// The curve the key is on
        crv: OkpCurve,
        /// The public key
        x: String,
    },
}

impl PublicKey {
    /// Returns the `kty` of this key
    pub const fn key_type(&self) -> &'static str {
        match self {
            Self::Ec { .. } => "EC",
            Self::Rsa { .. } => "RSA",
            Self::Okp { .. } => "OKP",
        }
    }

    /// Returns whether this key material can carry `algorithm`
    ///
    /// RFC 7518 Section 3.1 pins each algorithm to one key type, and the
    /// ECDSA ones to one curve as well: `ES256` is P-256 and nothing else.
    /// The HMAC algorithms are never carried by a public key - the secret
    /// that verifies them is the secret that signs.
    pub const fn supports(&self, algorithm: JwsAlgorithm) -> bool {
        matches!(
            (self, algorithm),
            (
                Self::Ec {
                    crv: EcCurve::P256,
                    ..
                },
                JwsAlgorithm::ES256
            ) | (
                Self::Ec {
                    crv: EcCurve::P384,
                    ..
                },
                JwsAlgorithm::ES384
            ) | (
                Self::Okp {
                    crv: OkpCurve::Ed25519,
                    ..
                },
                JwsAlgorithm::EdDSA
            ) | (
                Self::Rsa { .. },
                JwsAlgorithm::RS256
                    | JwsAlgorithm::RS384
                    | JwsAlgorithm::RS512
                    | JwsAlgorithm::PS256
                    | JwsAlgorithm::PS384
                    | JwsAlgorithm::PS512
            )
        )
    }
}

/// A JWS algorithm the key material cannot carry
///
/// Returned by [`PublicJwk::with_algorithm`] and by deserialization; see
/// [`PublicKey::supports`] for the pairings RFC 7518 Section 3.1 allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedAlgorithm {
    algorithm: JwsAlgorithm,
    key_type: &'static str,
}

impl UnsupportedAlgorithm {
    /// The algorithm that was rejected
    #[inline]
    pub fn algorithm(&self) -> JwsAlgorithm {
        self.algorithm
    }

    /// The `kty` of the key that cannot carry it
    #[inline]
    pub fn key_type(&self) -> &'static str {
        self.key_type
    }
}

impl std::fmt::Display for UnsupportedAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "a {} key cannot carry the {} algorithm",
            self.key_type, self.algorithm
        )
    }
}

impl std::error::Error for UnsupportedAlgorithm {}

/// A published public key (RFC 7517 Section 4)
///
/// Holds only the members of a *public* signing key: there is no way to
/// represent `d`, `p`, `q` or the other private members, so a key that goes
/// through this type cannot leak the secret half. Deserialization refuses a
/// document carrying them outright rather than quietly dropping them, and
/// likewise one declaring a use this key cannot serve - `use` other than
/// `sig`, or a `key_ops` that excludes `verify`.
///
/// # Example
/// ```
/// use volga_oauth_core::{JwsAlgorithm, jwk::{EcCurve, PublicJwk, PublicKey}};
///
/// let jwk = PublicJwk::new(PublicKey::Ec {
///         crv: EcCurve::P256,
///         x: "z9O8S-Itj6aJliZmUmWTG0Ko-GG23Wi6M3qbdjh5w-g".into(),
///         y: "nuk1MebXY11oQniSfOsKSnqmGjYQUhCHlwqeoeb6FDA".into(),
///     })
///     .with_key_id("2026-08")
///     .with_algorithm(JwsAlgorithm::ES256)
///     .unwrap();
///
/// let json = serde_json::to_value(&jwk).unwrap();
/// assert_eq!(json["kty"], "EC");
/// assert_eq!(json["crv"], "P-256");
/// assert_eq!(json["kid"], "2026-08");
/// assert_eq!(json["use"], "sig");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicJwk {
    #[serde(flatten)]
    key: PublicKey,

    #[serde(rename = "kid", skip_serializing_if = "Option::is_none")]
    key_id: Option<String>,

    #[serde(rename = "alg", skip_serializing_if = "Option::is_none")]
    algorithm: Option<JwsAlgorithm>,

    #[serde(rename = "use")]
    key_use: KeyUse,
}

impl PublicJwk {
    /// Creates a key from its public material
    pub fn new(key: PublicKey) -> Self {
        Self {
            key,
            key_id: None,
            algorithm: None,
            key_use: KeyUse::Signature,
        }
    }

    /// Sets the `kid` identifying this key among the others in a set
    ///
    /// A signature names the key it was made with by `kid`; without one, a
    /// verifier holding more than one key has to try them all.
    pub fn with_key_id(mut self, key_id: impl Into<String>) -> Self {
        self.key_id = Some(key_id.into());
        self
    }

    /// Sets the `alg` this key is used with
    ///
    /// Fails when the key material cannot carry it - an RSA key declaring
    /// `ES256`, a P-384 key declaring `ES256`, or any public key declaring
    /// an HMAC algorithm is a document no verifier can act on. See
    /// [`PublicKey::supports`].
    pub fn with_algorithm(mut self, algorithm: JwsAlgorithm) -> Result<Self, UnsupportedAlgorithm> {
        if !self.key.supports(algorithm) {
            return Err(UnsupportedAlgorithm {
                algorithm,
                key_type: self.key.key_type(),
            });
        }

        self.algorithm = Some(algorithm);
        Ok(self)
    }

    /// Returns the public key material
    #[inline]
    pub fn key(&self) -> &PublicKey {
        &self.key
    }

    /// Returns the `kid`, if set
    #[inline]
    pub fn key_id(&self) -> Option<&str> {
        self.key_id.as_deref()
    }

    /// Returns the `alg`, if set
    #[inline]
    pub fn algorithm(&self) -> Option<JwsAlgorithm> {
        self.algorithm
    }
}

/// The private and symmetric JWK members (RFC 7518 Sections 6.2.2, 6.3.2 and 6.4).
///
/// None of them belong in a published key, and none are representable by
/// [`PublicJwk`] - deserialization rejects a document carrying one so a
/// caller who reaches for the wrong file is told, rather than silently
/// publishing a key stripped of its secret.
const PRIVATE_MEMBERS: [&str; 8] = ["d", "p", "q", "dp", "dq", "qi", "oth", "k"];

impl<'de> Deserialize<'de> for PublicJwk {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // the public members, deserialized only after the document is
        // cleared of private ones
        #[derive(Deserialize)]
        struct Public {
            #[serde(flatten)]
            key: PublicKey,
            #[serde(rename = "kid")]
            key_id: Option<String>,
            #[serde(rename = "alg")]
            algorithm: Option<JwsAlgorithm>,
            #[serde(rename = "use", default)]
            key_use: KeyUse,
        }

        let document = serde_json::Value::deserialize(deserializer)?;
        if let Some(members) = document.as_object() {
            if let Some(member) = PRIVATE_MEMBERS
                .iter()
                .find(|member| members.contains_key(**member))
            {
                return Err(D::Error::custom(format!(
                    "the JWK carries the private member '{member}'; a published key holds \
                     public material only"
                )));
            }

            // RFC 7517 Section 4.3: `key_ops` is the other way a document
            // restricts what its key may be used for. This type publishes
            // signing keys and says so with `use: "sig"`, so a document
            // restricted away from verification has to be refused - dropping
            // the restriction and republishing it as a signing key would
            // turn an encryption-only key into a verification one
            if let Some(operations) = members.get("key_ops")
                && !operations
                    .as_array()
                    .is_some_and(|operations| operations.iter().any(|op| op == "verify"))
            {
                return Err(D::Error::custom(
                    "the JWK is restricted by `key_ops` to operations that exclude 'verify'; \
                     a published signing key has to permit signature verification",
                ));
            }
        }

        let public: Public = serde_json::from_value(document).map_err(D::Error::custom)?;

        // a declared `alg` the material cannot carry makes the whole
        // document unusable, so it is refused rather than round-tripped
        let jwk = Self::new(public.key);
        let jwk = match public.algorithm {
            Some(algorithm) => jwk.with_algorithm(algorithm).map_err(D::Error::custom)?,
            None => jwk,
        };

        Ok(Self {
            key_id: public.key_id,
            key_use: public.key_use,
            ..jwk
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ec_key() -> PublicKey {
        PublicKey::Ec {
            crv: EcCurve::P256,
            x: "z9O8S-Itj6aJliZmUmWTG0Ko-GG23Wi6M3qbdjh5w-g".into(),
            y: "nuk1MebXY11oQniSfOsKSnqmGjYQUhCHlwqeoeb6FDA".into(),
        }
    }

    #[test]
    fn it_serializes_every_key_type_per_rfc7518() {
        let cases = [
            (
                PublicKey::Ec {
                    crv: EcCurve::P384,
                    x: "x".into(),
                    y: "y".into(),
                },
                json!({"kty": "EC", "crv": "P-384", "x": "x", "y": "y", "use": "sig"}),
            ),
            (
                PublicKey::Rsa {
                    n: "n".into(),
                    e: "AQAB".into(),
                },
                json!({"kty": "RSA", "n": "n", "e": "AQAB", "use": "sig"}),
            ),
            (
                PublicKey::Okp {
                    crv: OkpCurve::Ed25519,
                    x: "x".into(),
                },
                json!({"kty": "OKP", "crv": "Ed25519", "x": "x", "use": "sig"}),
            ),
        ];

        for (key, expected) in cases {
            let jwk = PublicJwk::new(key);
            assert_eq!(serde_json::to_value(&jwk).unwrap(), expected);
            // ...and every one of them round-trips
            assert_eq!(serde_json::from_value::<PublicJwk>(expected).unwrap(), jwk);
        }
    }

    #[test]
    fn it_carries_the_optional_members() {
        let jwk = PublicJwk::new(ec_key())
            .with_key_id("2026-08")
            .with_algorithm(JwsAlgorithm::ES256)
            .unwrap();
        assert_eq!(jwk.key_id(), Some("2026-08"));
        assert_eq!(jwk.algorithm(), Some(JwsAlgorithm::ES256));
        assert!(matches!(jwk.key(), PublicKey::Ec { .. }));

        let json = serde_json::to_value(&jwk).unwrap();
        assert_eq!(json["kid"], "2026-08");
        assert_eq!(json["alg"], "ES256");
        assert_eq!(serde_json::from_value::<PublicJwk>(json).unwrap(), jwk);

        // absent optional members stay absent rather than serializing null
        let bare = serde_json::to_value(PublicJwk::new(ec_key())).unwrap();
        assert!(bare.get("kid").is_none() && bare.get("alg").is_none());
    }

    #[test]
    fn it_refuses_a_private_key() {
        for member in PRIVATE_MEMBERS {
            let mut document = serde_json::to_value(PublicJwk::new(ec_key())).unwrap();
            document[member] = "the-secret".into();
            let err = serde_json::from_value::<PublicJwk>(document)
                .unwrap_err()
                .to_string();
            assert!(err.contains(member), "{member} was not refused: {err}");
        }
    }

    #[test]
    fn it_pairs_algorithms_with_the_material_that_carries_them() {
        let ec384 = PublicKey::Ec {
            crv: EcCurve::P384,
            x: "x".into(),
            y: "y".into(),
        };
        let rsa = PublicKey::Rsa {
            n: "n".into(),
            e: "AQAB".into(),
        };
        let okp = PublicKey::Okp {
            crv: OkpCurve::Ed25519,
            x: "x".into(),
        };

        // RFC 7518 Section 3.1 pins each algorithm to one key type, and the
        // ECDSA ones to one curve
        assert!(ec_key().supports(JwsAlgorithm::ES256));
        assert!(!ec_key().supports(JwsAlgorithm::ES384));
        assert!(ec384.supports(JwsAlgorithm::ES384));
        assert!(!ec384.supports(JwsAlgorithm::ES256));
        assert!(okp.supports(JwsAlgorithm::EdDSA));
        for rsa_alg in [
            JwsAlgorithm::RS256,
            JwsAlgorithm::RS384,
            JwsAlgorithm::RS512,
            JwsAlgorithm::PS256,
            JwsAlgorithm::PS384,
            JwsAlgorithm::PS512,
        ] {
            assert!(rsa.supports(rsa_alg));
            assert!(!ec_key().supports(rsa_alg));
        }

        // no public key carries an HMAC algorithm - the secret that
        // verifies one is the secret that signs it
        for hmac in [
            JwsAlgorithm::HS256,
            JwsAlgorithm::HS384,
            JwsAlgorithm::HS512,
        ] {
            for key in [ec_key(), rsa.clone(), okp.clone()] {
                assert!(!key.supports(hmac), "{} accepted {hmac}", key.key_type());
            }
        }

        // ...and the builder refuses what the material cannot carry
        let err = PublicJwk::new(rsa.clone())
            .with_algorithm(JwsAlgorithm::ES256)
            .unwrap_err();
        assert_eq!(err.key_type(), "RSA");
        assert_eq!(err.algorithm(), JwsAlgorithm::ES256);
        assert_eq!(
            err.to_string(),
            "a RSA key cannot carry the ES256 algorithm"
        );
        assert!(
            PublicJwk::new(rsa)
                .with_algorithm(JwsAlgorithm::RS256)
                .is_ok()
        );
    }

    #[test]
    fn it_refuses_a_declared_algorithm_the_key_cannot_carry() {
        // the same rule on the way in: a document pairing an algorithm with
        // material that cannot carry it is unusable, whoever wrote it
        for document in [
            json!({"kty": "RSA", "n": "n", "e": "AQAB", "alg": "ES256"}),
            json!({"kty": "EC", "crv": "P-384", "x": "x", "y": "y", "alg": "ES256"}),
            json!({"kty": "EC", "crv": "P-256", "x": "x", "y": "y", "alg": "HS256"}),
            json!({"kty": "OKP", "crv": "Ed25519", "x": "x", "alg": "ES256"}),
        ] {
            let err = serde_json::from_value::<PublicJwk>(document.clone())
                .unwrap_err()
                .to_string();
            assert!(err.contains("cannot carry"), "accepted {document}: {err}");
        }

        // the matching pairs, and an omitted `alg`, go through
        for document in [
            json!({"kty": "RSA", "n": "n", "e": "AQAB", "alg": "PS512"}),
            json!({"kty": "EC", "crv": "P-384", "x": "x", "y": "y", "alg": "ES384"}),
            json!({"kty": "OKP", "crv": "Ed25519", "x": "x", "alg": "EdDSA"}),
            json!({"kty": "EC", "crv": "P-256", "x": "x", "y": "y"}),
        ] {
            assert!(
                serde_json::from_value::<PublicJwk>(document.clone()).is_ok(),
                "refused {document}"
            );
        }
    }

    #[test]
    fn it_refuses_documents_it_cannot_publish() {
        // a key type or curve this crate does not sign with
        for document in [
            json!({"kty": "oct", "k": "c2VjcmV0"}),
            json!({"kty": "EC", "crv": "P-521", "x": "x", "y": "y"}),
            json!({"kty": "EC", "crv": "P-256", "x": "x"}),
            json!({"kty": "RSA", "n": "n"}),
            // the curve has to belong to the key type: an EC key is never
            // on an Edwards curve, and an OKP key is never on a NIST one
            json!({"kty": "EC", "crv": "Ed25519", "x": "x", "y": "y"}),
            json!({"kty": "OKP", "crv": "P-256", "x": "x"}),
            // ...and X25519 is for key agreement, never for signing
            json!({"kty": "OKP", "crv": "X25519", "x": "x"}),
        ] {
            assert!(
                serde_json::from_value::<PublicJwk>(document.clone()).is_err(),
                "accepted {document}"
            );
        }

        // an encryption key is not a signing key
        let mut document = serde_json::to_value(PublicJwk::new(ec_key())).unwrap();
        document["use"] = "enc".into();
        assert!(serde_json::from_value::<PublicJwk>(document).is_err());

        // unrecognized public members are tolerated for forward compatibility
        let mut document = serde_json::to_value(PublicJwk::new(ec_key())).unwrap();
        document["x5t#S256"] = "thumbprint".into();
        assert!(serde_json::from_value::<PublicJwk>(document).is_ok());
    }

    #[test]
    fn it_refuses_a_key_restricted_away_from_verification() {
        let with_ops = |operations: serde_json::Value| {
            let mut document = serde_json::to_value(PublicJwk::new(ec_key())).unwrap();
            document["key_ops"] = operations;
            serde_json::from_value::<PublicJwk>(document)
        };

        // dropping the restriction and republishing under `use: "sig"`
        // would turn an encryption-only key into a verification one
        for operations in [
            json!(["encrypt"]),
            json!(["decrypt", "unwrapKey"]),
            json!(["deriveKey"]),
            // a published key that only signs is nonsense too
            json!(["sign"]),
            json!([]),
            // a malformed restriction is not evidence of permission
            json!("verify"),
            json!(null),
        ] {
            let err = with_ops(operations.clone()).unwrap_err().to_string();
            assert!(err.contains("key_ops"), "accepted {operations}: {err}");
        }

        // ...while anything permitting verification goes through
        for operations in [json!(["verify"]), json!(["sign", "verify"])] {
            assert!(with_ops(operations.clone()).is_ok(), "refused {operations}");
        }
    }

    #[test]
    fn it_builds_and_searches_a_set() {
        let set = JwkSet::new([
            PublicJwk::new(ec_key()).with_key_id("old"),
            PublicJwk::new(ec_key()).with_key_id("current"),
        ]);
        assert_eq!(set.find("current").unwrap().key_id(), Some("current"));
        assert!(set.find("missing").is_none());
        assert!(JwkSet::default().keys.is_empty());

        let json = serde_json::to_value(&set).unwrap();
        assert_eq!(json["keys"][1]["kid"], "current");
        assert_eq!(serde_json::from_value::<JwkSet>(json).unwrap(), set);
    }
}

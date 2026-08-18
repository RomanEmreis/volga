//! Shared JWS signing plumbing
//!
//! The two things this crate signs - a `private_key_jwt` client assertion
//! (RFC 7523 Section 2.2) and a DPoP proof (RFC 9449 Section 4) - are
//! different objects with different lifetimes, but they load a key the same
//! way: an asymmetric algorithm names a key family, and the family selects
//! the constructor a PEM or DER blob goes through. That mapping lives here
//! so the two cannot drift apart.

use jsonwebtoken::{Algorithm, EncodingKey};
use volga_oauth_core::pem::PemKind;

use crate::{ClientError, JwsAlgorithm};

/// The key families a [`JwsAlgorithm`] can be keyed by, selecting the
/// constructor a PEM or DER blob is loaded with.
#[derive(Clone, Copy)]
pub(crate) enum KeyFamily {
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
/// The symmetric algorithms have no place here: both objects this crate
/// signs prove possession of a key the recipient does not hold, and an
/// HMAC secret proves nothing of the sort.
pub(crate) fn asymmetric(algorithm: JwsAlgorithm) -> Result<(Algorithm, KeyFamily), ClientError> {
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
                "{symmetric} is a shared-secret algorithm; signing here proves possession of \
                 an asymmetric key the recipient does not hold"
            )));
        }
    })
}

/// Loads a PEM-encoded private key for `algorithm`, refusing a header that
/// positively names the wrong key family before the parser sees it.
pub(crate) fn from_pem(
    pem: &[u8],
    algorithm: JwsAlgorithm,
) -> Result<(EncodingKey, Algorithm), ClientError> {
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

    Ok((key, alg))
}

/// Adopts a DER-encoded private key for `algorithm`
///
/// Unlike [`from_pem`] the bytes are taken as-is; a key that does not match
/// the algorithm surfaces at the first signing attempt rather than here.
pub(crate) fn from_der(
    der: &[u8],
    algorithm: JwsAlgorithm,
) -> Result<(EncodingKey, Algorithm), ClientError> {
    let (alg, family) = asymmetric(algorithm)?;
    let key = match family {
        KeyFamily::Rsa => EncodingKey::from_rsa_der(der),
        KeyFamily::Ec => EncodingKey::from_ec_der(der),
        KeyFamily::Ed => EncodingKey::from_ed_der(der),
    };

    Ok((key, alg))
}

/// Reads the PEM file at `path` for [`from_pem`], naming it in the error
/// when it cannot be read.
pub(crate) fn read_pem_file(path: &std::path::Path) -> Result<Vec<u8>, ClientError> {
    std::fs::read(path).map_err(|err| {
        ClientError::signing(format!(
            "failed to read the signing key at {}: {err}",
            path.display()
        ))
    })
}

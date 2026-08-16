//! JWS signing and verification algorithm identifiers

use std::fmt::{Display, Formatter};

/// A JWS signing / verification algorithm (RFC 7518 Section 3.1)
///
/// The `alg` header value of a JWT, shared by everything in the framework
/// that signs or verifies one: bearer authentication on the server side,
/// and `private_key_jwt` client assertions on the client side. The mapping
/// onto the underlying JWT implementation stays private to each crate, so
/// no consumer has to name it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
#[non_exhaustive]
pub enum JwsAlgorithm {
    /// HMAC using SHA-256
    #[default]
    HS256,
    /// HMAC using SHA-384
    HS384,
    /// HMAC using SHA-512
    HS512,
    /// ECDSA using P-256 and SHA-256
    ES256,
    /// ECDSA using P-384 and SHA-384
    ES384,
    /// RSASSA-PKCS1-v1_5 using SHA-256
    RS256,
    /// RSASSA-PKCS1-v1_5 using SHA-384
    RS384,
    /// RSASSA-PKCS1-v1_5 using SHA-512
    RS512,
    /// RSASSA-PSS using SHA-256
    PS256,
    /// RSASSA-PSS using SHA-384
    PS384,
    /// RSASSA-PSS using SHA-512
    PS512,
    /// Edwards-curve Digital Signature Algorithm (EdDSA)
    EdDSA,
}

impl JwsAlgorithm {
    /// Returns the registered `alg` header value of this algorithm
    ///
    /// Use it to match against the algorithm lists a server advertises,
    /// such as `token_endpoint_auth_signing_alg_values_supported`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::HS256 => "HS256",
            Self::HS384 => "HS384",
            Self::HS512 => "HS512",
            Self::ES256 => "ES256",
            Self::ES384 => "ES384",
            Self::RS256 => "RS256",
            Self::RS384 => "RS384",
            Self::RS512 => "RS512",
            Self::PS256 => "PS256",
            Self::PS384 => "PS384",
            Self::PS512 => "PS512",
            Self::EdDSA => "EdDSA",
        }
    }

    /// Returns `true` for the HMAC algorithms, where signer and verifier
    /// hold the same secret
    ///
    /// Anything built on proving possession of a private key - a
    /// `private_key_jwt` client assertion, a key published in a JWKS -
    /// must refuse these: a shared secret proves nothing about who signed.
    pub const fn is_symmetric(&self) -> bool {
        matches!(self, Self::HS256 | Self::HS384 | Self::HS512)
    }
}

impl Display for JwsAlgorithm {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [JwsAlgorithm; 12] = [
        JwsAlgorithm::HS256,
        JwsAlgorithm::HS384,
        JwsAlgorithm::HS512,
        JwsAlgorithm::ES256,
        JwsAlgorithm::ES384,
        JwsAlgorithm::RS256,
        JwsAlgorithm::RS384,
        JwsAlgorithm::RS512,
        JwsAlgorithm::PS256,
        JwsAlgorithm::PS384,
        JwsAlgorithm::PS512,
        JwsAlgorithm::EdDSA,
    ];

    #[test]
    fn it_defaults_to_hs256() {
        assert_eq!(JwsAlgorithm::default(), JwsAlgorithm::HS256);
    }

    #[test]
    fn it_renders_the_registered_alg_names() {
        // the `alg` name is compared against server-advertised strings, so
        // it must match the RFC 7518 spelling exactly, `EdDSA` included
        for alg in ALL {
            assert_eq!(alg.to_string(), alg.as_str());
            assert_eq!(format!("{alg:?}"), alg.as_str());
        }
        assert_eq!(JwsAlgorithm::EdDSA.as_str(), "EdDSA");
    }

    #[test]
    fn it_separates_symmetric_from_asymmetric() {
        for alg in ALL {
            assert_eq!(alg.is_symmetric(), alg.as_str().starts_with("HS"));
        }
    }
}

//! JWT signing/verifying algorithm identifiers.

/// A JWT signing/verifying algorithm.
///
/// Mirrors the algorithms defined in [RFC 7518](https://www.rfc-editor.org/rfc/rfc7518).
/// Use this type with [`BearerAuthConfig::with_alg`](super::bearer::BearerAuthConfig::with_alg).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum Algorithm {
    /// HMAC using SHA-256.
    HS256,
    /// HMAC using SHA-384.
    HS384,
    /// HMAC using SHA-512.
    HS512,
    /// ECDSA using P-256 and SHA-256.
    ES256,
    /// ECDSA using P-384 and SHA-384.
    ES384,
    /// RSASSA-PKCS1-v1_5 using SHA-256.
    RS256,
    /// RSASSA-PKCS1-v1_5 using SHA-384.
    RS384,
    /// RSASSA-PKCS1-v1_5 using SHA-512.
    RS512,
    /// RSASSA-PSS using SHA-256.
    PS256,
    /// RSASSA-PSS using SHA-384.
    PS384,
    /// RSASSA-PSS using SHA-512.
    PS512,
    /// Edwards-curve Digital Signature Algorithm (EdDSA).
    EdDSA,
}

impl Default for Algorithm {
    #[inline]
    fn default() -> Self {
        Algorithm::HS256
    }
}

impl From<Algorithm> for jsonwebtoken::Algorithm {
    #[inline]
    fn from(value: Algorithm) -> Self {
        match value {
            Algorithm::HS256 => jsonwebtoken::Algorithm::HS256,
            Algorithm::HS384 => jsonwebtoken::Algorithm::HS384,
            Algorithm::HS512 => jsonwebtoken::Algorithm::HS512,
            Algorithm::ES256 => jsonwebtoken::Algorithm::ES256,
            Algorithm::ES384 => jsonwebtoken::Algorithm::ES384,
            Algorithm::RS256 => jsonwebtoken::Algorithm::RS256,
            Algorithm::RS384 => jsonwebtoken::Algorithm::RS384,
            Algorithm::RS512 => jsonwebtoken::Algorithm::RS512,
            Algorithm::PS256 => jsonwebtoken::Algorithm::PS256,
            Algorithm::PS384 => jsonwebtoken::Algorithm::PS384,
            Algorithm::PS512 => jsonwebtoken::Algorithm::PS512,
            Algorithm::EdDSA => jsonwebtoken::Algorithm::EdDSA,
        }
    }
}

impl From<jsonwebtoken::Algorithm> for Algorithm {
    #[inline]
    fn from(value: jsonwebtoken::Algorithm) -> Self {
        match value {
            jsonwebtoken::Algorithm::HS256 => Algorithm::HS256,
            jsonwebtoken::Algorithm::HS384 => Algorithm::HS384,
            jsonwebtoken::Algorithm::HS512 => Algorithm::HS512,
            jsonwebtoken::Algorithm::ES256 => Algorithm::ES256,
            jsonwebtoken::Algorithm::ES384 => Algorithm::ES384,
            jsonwebtoken::Algorithm::RS256 => Algorithm::RS256,
            jsonwebtoken::Algorithm::RS384 => Algorithm::RS384,
            jsonwebtoken::Algorithm::RS512 => Algorithm::RS512,
            jsonwebtoken::Algorithm::PS256 => Algorithm::PS256,
            jsonwebtoken::Algorithm::PS384 => Algorithm::PS384,
            jsonwebtoken::Algorithm::PS512 => Algorithm::PS512,
            jsonwebtoken::Algorithm::EdDSA => Algorithm::EdDSA,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_defaults_to_hs256() {
        assert_eq!(Algorithm::default(), Algorithm::HS256);
    }

    #[test]
    fn it_converts_every_variant_to_jsonwebtoken() {
        let pairs: [(Algorithm, jsonwebtoken::Algorithm); 12] = [
            (Algorithm::HS256, jsonwebtoken::Algorithm::HS256),
            (Algorithm::HS384, jsonwebtoken::Algorithm::HS384),
            (Algorithm::HS512, jsonwebtoken::Algorithm::HS512),
            (Algorithm::ES256, jsonwebtoken::Algorithm::ES256),
            (Algorithm::ES384, jsonwebtoken::Algorithm::ES384),
            (Algorithm::RS256, jsonwebtoken::Algorithm::RS256),
            (Algorithm::RS384, jsonwebtoken::Algorithm::RS384),
            (Algorithm::RS512, jsonwebtoken::Algorithm::RS512),
            (Algorithm::PS256, jsonwebtoken::Algorithm::PS256),
            (Algorithm::PS384, jsonwebtoken::Algorithm::PS384),
            (Algorithm::PS512, jsonwebtoken::Algorithm::PS512),
            (Algorithm::EdDSA, jsonwebtoken::Algorithm::EdDSA),
        ];
        for (volga, jwt) in pairs {
            assert_eq!(jsonwebtoken::Algorithm::from(volga), jwt);
            assert_eq!(Algorithm::from(jwt), volga);
        }
    }

    #[test]
    fn it_debugs_hs256() {
        assert_eq!(format!("{:?}", Algorithm::HS256), "HS256");
    }

    #[test]
    fn it_compares_for_equality() {
        assert_eq!(Algorithm::RS256, Algorithm::RS256);
        assert_ne!(Algorithm::RS256, Algorithm::HS256);
    }
}

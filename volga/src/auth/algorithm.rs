//! JWT signing/verifying algorithm identifiers.

/// A JWT signing/verifying algorithm.
///
/// Mirrors the algorithms defined in [RFC 7518](https://www.rfc-editor.org/rfc/rfc7518).
/// Use this type with [`BearerAuthConfig::with_alg`](super::bearer::BearerAuthConfig::with_alg).
///
/// The same type is used by the OAuth client crates, so a `private_key_jwt`
/// client assertion and a bearer token this server issues are described in
/// one vocabulary.
pub use volga_oauth_core::JwsAlgorithm as Algorithm;

/// Converts an [`Algorithm`] to the underlying `jsonwebtoken::Algorithm`.
///
/// A free function rather than an inherent method: the type itself is
/// shared through `volga-oauth-core`, which carries no cryptography.
/// Crate-private to keep `jsonwebtoken` out of volga's public API surface.
///
/// # Panics
/// Panics on an algorithm this mapping does not cover. `JwsAlgorithm` is
/// `#[non_exhaustive]`, so a variant added to `volga-oauth-core` without a
/// mapping here cannot be caught at compile time from this side. Every
/// variant that exists today is covered, so this is unreachable in a
/// consistent build - and substituting a default instead would be worse
/// than a crash: the caller's algorithm would stay disabled while a
/// different one was quietly accepted for verifying tokens.
#[inline]
pub(crate) fn to_jwt(alg: Algorithm) -> jsonwebtoken::Algorithm {
    match alg {
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
        // fail closed: an unmapped algorithm is this workspace being
        // internally inconsistent, and the token validation policy is not
        // something to guess at
        unmapped => panic!(
            "volga has no mapping for the {unmapped} JWS algorithm; this is a volga bug, \
             please report it"
        ),
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
            assert_eq!(to_jwt(volga), jwt);
            // the shared `alg` name and the underlying one must agree, or
            // a token would be signed under a header it does not match
            assert_eq!(volga.as_str(), format!("{jwt:?}"));
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

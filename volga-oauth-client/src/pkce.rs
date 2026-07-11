//! PKCE support (RFC 7636)
//!
//! OAuth 2.1 requires every Authorization Code request to carry a PKCE
//! challenge. Only the `S256` method is provided — `plain` was removed
//! from OAuth 2.1 and offers no protection worth keeping.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

/// The PKCE code challenge method sent in authorization requests;
/// OAuth 2.1 permits only `S256`
pub const PKCE_METHOD: &str = "S256";

/// A PKCE code verifier and its `S256` code challenge (RFC 7636 §4.1–4.2)
///
/// Created by [`Pkce::new`] with a fresh 256-bit random verifier. The
/// challenge goes into the authorization request, the verifier into the
/// subsequent token request — [`OAuthClient`](crate::OAuthClient) wires
/// both automatically.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Default for Pkce {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Pkce {
    /// Generates a fresh pair: a 256-bit random code verifier and its
    /// `S256` challenge
    pub fn new() -> Self {
        Self::from_verifier(random_urlsafe(32))
    }

    fn from_verifier(verifier: String) -> Self {
        let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(digest.as_ref());
        Self {
            verifier,
            challenge,
        }
    }

    /// Returns the `code_verifier` sent to the token endpoint
    #[inline]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// Returns the `code_challenge` sent in the authorization request
    #[inline]
    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

impl std::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // the verifier is a credential — never expose it in debug output
        f.debug_struct("Pkce")
            .field("verifier", &"[redacted]")
            .field("challenge", &self.challenge)
            .finish()
    }
}

/// Returns `len` random bytes as a base64url string without padding.
pub(crate) fn random_urlsafe(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    aws_lc_rs::rand::fill(&mut bytes).expect("system RNG failure");
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_computes_the_rfc7636_reference_challenge() {
        // RFC 7636 Appendix B
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into());
        assert_eq!(
            pkce.challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn it_generates_unique_spec_compliant_verifiers() {
        let (a, b) = (Pkce::new(), Pkce::new());
        assert_ne!(a.verifier(), b.verifier());
        // RFC 7636 §4.1: 43–128 characters from the unreserved set
        assert_eq!(a.verifier().len(), 43);
        assert!(
            a.verifier()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        );
    }

    #[test]
    fn it_redacts_the_verifier_in_debug_output() {
        let pkce = Pkce::new();
        let debug = format!("{pkce:?}");
        assert!(!debug.contains(pkce.verifier()));
        assert!(debug.contains(pkce.challenge()));
    }
}

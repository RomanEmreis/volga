//! Owned wrapper around `jsonwebtoken::DecodingKey`.

use crate::auth::key::jwt_key;

jwt_key! {
    /// A key used to verify JWTs.
    ///
    /// Wraps an internal verification key. Use one of the `from_*` or
    /// `try_from_*` constructors to build an instance, then pass it to
    /// [`BearerAuthConfig::set_decoding_key`](super::bearer::BearerAuthConfig::set_decoding_key).
    ///
    /// Every constructor below is shared verbatim with
    /// [`EncodingKey`](super::EncodingKey), which loads the other half of
    /// the same key material.
    DecodingKey, DecodingKey, kind = "verification", half = "public"
}

impl DecodingKey {
    /// Builds a verification key from a JSON Web Key (RFC 7517), the form
    /// an issuer publishes its keys in
    ///
    /// Takes one key as JSON text, such as an entry of the `keys` array of
    /// an issuer's JWKS: RSA, EC, Ed25519 (`OKP`) or a symmetric `oct` key.
    /// The key's own `alg`, `use` and `key_ops` are not read: the algorithm
    /// tokens must be signed with comes from
    /// [`BearerAuthConfig::with_alg`](super::bearer::BearerAuthConfig::with_alg),
    /// as for a PEM key.
    ///
    /// A key loaded this way does not follow the issuer's rotations. To
    /// track them, fetch the whole set with `App::use_oauth` and
    /// `OAuthConfig::with_jwks_uri` (feature `oauth-client`) instead.
    ///
    /// Returns an error if the text is not a JSON Web Key, or names a key
    /// type or curve that is not supported.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::DecodingKey;
    ///
    /// let key = DecodingKey::from_jwk(
    ///     r#"{"kty":"OKP","crv":"Ed25519","x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"}"#,
    /// )?;
    /// # Ok::<(), volga::error::Error>(())
    /// ```
    pub fn from_jwk(jwk: &str) -> Result<Self, crate::error::Error> {
        let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_str(jwk)
            .map_err(|err| crate::error::Error::from_jwt_error(err.into()))?;

        jsonwebtoken::DecodingKey::from_jwk(&jwk)
            .map(Self)
            .map_err(crate::error::Error::from_jwt_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SPKI-format RSA public key matching the private key used in EncodingKey tests.
    const RSA_PUBLIC_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAq1ma/MoK5uWwsPxUNsVH
1e+ybz/TzUGiFqUKbYkLTpXr9kpXi0i5SZOkGXHnLz1ch4gmOMuvvoLNwRyBzZGk
OOd8IoLZAe4OAdmpQ2T0pY6szvUCK3WpIa06P7n20msOuc8bzm6CFM9fJU5/vHze
LGAj4Vi2GoFz4Lm3zUlZcY2zQWu2kdJZt6HbAM4s+nv1m3gqX+m5gTOjBP7oxEdN
sOGZnl5v8h8uZ/U+CP2emvr67HW+Pph8OjVvXbyhBNGAbEljoXjJMLcqB5ULxXC4
AspE+EfAZD5pCQO2ssUVPjw07qLNFd6gTJ7q41k2bNrS/SmYqWMeWttwEGS5Tjm3
XwIDAQAB
-----END PUBLIC KEY-----
";

    crate::auth::key::jwt_key_tests!(DecodingKey, pem = RSA_PUBLIC_PEM, slug = "decoding");

    // Throwaway P-256 private key (PKCS#8), generated for this test only.
    const EC_PRIVATE_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/mhf18fVqZlAZPBZ
Ri7LHYo8m45nXTbQqcY067dsRB+hRANCAARWY6oRJMNMJ3Ozc8nyIwZ1nIcVFUHa
oP/qAYIpT1laaG8pxhqNdYYq6wtBut8vKlP9Urf2GMy7Bu5o3Bh6+pQ3
-----END PRIVATE KEY-----
";

    fn verifies(key: &DecodingKey, token: &str, alg: jsonwebtoken::Algorithm) -> bool {
        let mut validation = jsonwebtoken::Validation::new(alg);
        validation.required_spec_claims.clear();
        jsonwebtoken::decode::<serde_json::Value>(token, &key.0, &validation).is_ok()
    }

    #[test]
    fn it_verifies_tokens_with_a_key_from_a_jwk() {
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::Jwk};

        let signing = EncodingKey::from_ec_pem(EC_PRIVATE_PEM).unwrap();
        let jwk = Jwk::from_encoding_key(&signing, Algorithm::ES256).unwrap();
        let key = DecodingKey::from_jwk(&serde_json::to_string(&jwk).unwrap()).unwrap();

        let token = encode(
            &Header::new(Algorithm::ES256),
            &serde_json::json!({}),
            &signing,
        )
        .unwrap();
        assert!(verifies(&key, &token, Algorithm::ES256));

        // the same signature over another payload ({"sub":"x"}) does not
        // verify: the key really checks the signature
        let (signed, signature) = token.rsplit_once('.').unwrap();
        let (header, _) = signed.split_once('.').unwrap();
        let forged = format!("{header}.eyJzdWIiOiJ4In0.{signature}");
        assert!(!verifies(&key, &forged, Algorithm::ES256));
    }

    #[test]
    fn it_reads_a_symmetric_jwk() {
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

        // base64url("secret")
        let key = DecodingKey::from_jwk(r#"{"kty":"oct","k":"c2VjcmV0"}"#).unwrap();
        let token = encode(
            &Header::new(Algorithm::HS256),
            &serde_json::json!({}),
            &EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        assert!(verifies(&key, &token, Algorithm::HS256));
    }

    #[test]
    fn it_fails_on_what_is_not_a_jwk() {
        // not JSON
        assert!(DecodingKey::from_jwk("not a jwk").is_err());
        // a JWKS rather than one of its keys
        assert!(DecodingKey::from_jwk(r#"{"keys":[]}"#).is_err());
        // an RSA key without its components
        assert!(DecodingKey::from_jwk(r#"{"kty":"RSA"}"#).is_err());
        // a key type jsonwebtoken does not know
        assert!(DecodingKey::from_jwk(r#"{"kty":"PQC","pub":"AAAA"}"#).is_err());
    }
}

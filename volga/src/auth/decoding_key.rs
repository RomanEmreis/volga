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
}

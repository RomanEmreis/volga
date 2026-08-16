//! Owned wrapper around `jsonwebtoken::EncodingKey`.

use crate::auth::key::jwt_key;

jwt_key! {
    /// A key used to sign JWTs.
    ///
    /// Wraps an internal signing key. Use one of the `from_*` or `try_from_*`
    /// constructors to build an instance, then pass it to
    /// [`BearerAuthConfig::set_encoding_key`](super::bearer::BearerAuthConfig::set_encoding_key).
    ///
    /// Every constructor below is shared verbatim with
    /// [`DecodingKey`](super::DecodingKey), which loads the other half of
    /// the same key material.
    EncodingKey, EncodingKey, kind = "signing", half = "private"
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sample RSA private key in PEM format, generated for tests only.
    const RSA_PRIVATE_PEM: &[u8] = b"-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAq1ma/MoK5uWwsPxUNsVH1e+ybz/TzUGiFqUKbYkLTpXr9kpX
i0i5SZOkGXHnLz1ch4gmOMuvvoLNwRyBzZGkOOd8IoLZAe4OAdmpQ2T0pY6szvUC
K3WpIa06P7n20msOuc8bzm6CFM9fJU5/vHzeLGAj4Vi2GoFz4Lm3zUlZcY2zQWu2
kdJZt6HbAM4s+nv1m3gqX+m5gTOjBP7oxEdNsOGZnl5v8h8uZ/U+CP2emvr67HW+
Pph8OjVvXbyhBNGAbEljoXjJMLcqB5ULxXC4AspE+EfAZD5pCQO2ssUVPjw07qLN
Fd6gTJ7q41k2bNrS/SmYqWMeWttwEGS5Tjm3XwIDAQABAoIBABhmQZmjnCtmoO9B
IaR5sstJvAoLIbVnJ0QjSvfMdtpzKdk5lwD9KjZnbbFgWqZphRSXVnzKMEHh/9/E
8qPf78ToNx21FrcwHsTkXmjjAKFjbL+oRFfDRkZZZAY+CxpvBQ4LGJWyBvXwz6jb
BVyppnmpZ8L+LCY5fwaYMQ8I0ExD7akqjEgMo9QTNpGeHl1hIVlbn93c/8MQyLpk
OxHcq8DhRCTYQsEc7D8z7wU7QhKw0Wf1FjUDkSC4LVIVFEbKp8EqOKJpoPJsfj1r
CiF8Vy8AUBIN1pC5nOsu3L6l1aONmDlq7ufVg2M2odZzOXvUQlzQGP3b10f1JRck
O+lCqeECgYEA3TgcuopwW9DYYlHhjaSDRy7EZ5xD50fWyHCH9SL2H1qwz8Jsf9u8
rFT/L5aEWW2xoBN7YLXHnALxFgZtEcqW9NEpkU8Uii7ZYO1NU+HhbeNE27a2jZzg
DI4HfDckajKNmn/y+2JGsmvCqwAPmYj5qvCBfZZmxBc6Zeq6cFaZJ/8CgYEAxeDl
ItlCmHVGsW94Kcm3f1FTaGvVptHB9xftxiGm/Xdkw70dRuZPsprBE8A7MwhZ8afk
FVxLoTGEk7wuuwSpYyngJ4/+SdlH4xXz5Bgr07dqKwXAS9AWUhNU9YYMmbkI5Rjk
MuAeBF7XS8nzrlXvHrfnajn9Pq3UeL8AUv7jhuECgYB6e4uqMpDnfh8NuPNwZ4/H
FkRZUHMnjUPQb4TGCjVSbIJmAcRBPHqsfBeqH0qrfA05Ua+tcRSKPPcOtU1zDAW9
uTJj2P0pDkF6bl2ZxiPcQt3IwF8CcAlqhFSVb+nZ/CokcnBA5vVJLSJv5FyKbAOb
dlGANmy5ZzE5NobWwkuCkwKBgExZLlkx24dOdfyaXBWK3Osc+Wy4BQLH9VcWZrlC
Xfxu7ajTS31O4yojk+XuPCu95ouMLNbJfEWDLpu8MGmYG1EhI6pn7UGFJ4MCFQHV
5VhcImpMFB6hw00FRWhJ7Bt5pvM3bTGfe6Ue0AFBzcM+KSz9yIDiDoXLRT9jmP1v
dL3hAoGBAJrnfhTQ6tSUmdBkgk6SfNx+RgPRj/7IbHlP1UYNS1i2OmhH+5T8qVZx
DdAfI6OjB86GKnRAtfRfPxJqT7vV6m6pGXyGcJyPdFINbENx31LXV6E7aXJEJbQX
JUI7cp++yw7jYS/V9fAJTMjs/uk1dRuXRoWbwc4o+PlhcBtU2VAp
-----END RSA PRIVATE KEY-----
";

    crate::auth::key::jwt_key_tests!(EncodingKey, pem = RSA_PRIVATE_PEM, slug = "encoding");
}

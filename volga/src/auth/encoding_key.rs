//! Owned wrapper around `jsonwebtoken::EncodingKey`.

use crate::error::Error;

pub(crate) fn read_env_var(name: &str) -> Result<String, Error> {
    match std::env::var(name) {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => {
            Err(Error::server_error(format!("{name} env var is not set")))
        }
        Err(std::env::VarError::NotUnicode(_)) => Err(Error::server_error(format!(
            "{name} env var is not valid UTF-8"
        ))),
    }
}

pub(crate) fn read_key_file(path: &std::path::Path) -> Result<Vec<u8>, Error> {
    std::fs::read(path).map_err(|e| {
        Error::server_error(format!("Failed to read key file {}: {e}", path.display()))
    })
}

/// A key used to sign JWTs.
///
/// Wraps an internal signing key. Use one of the `from_*` or `try_from_*`
/// constructors to build an instance, then pass it to
/// [`BearerAuthConfig::set_encoding_key`](super::bearer::BearerAuthConfig::set_encoding_key).
pub struct EncodingKey(pub(crate) jsonwebtoken::EncodingKey);

impl std::fmt::Debug for EncodingKey {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncodingKey([redacted])")
    }
}

impl EncodingKey {
    /// Builds an HMAC key from a raw byte slice.
    ///
    /// Use with `HS256`, `HS384`, or `HS512`.
    #[inline]
    pub fn from_secret(secret: &[u8]) -> Self {
        Self(jsonwebtoken::EncodingKey::from_secret(secret))
    }

    /// Builds an HMAC key by base64-decoding a string.
    ///
    /// Returns an error if the input is not valid base64.
    #[inline]
    pub fn from_base64_secret(secret: &str) -> Result<Self, Error> {
        jsonwebtoken::EncodingKey::from_base64_secret(secret)
            .map(Self)
            .map_err(Error::from_jwt_error)
    }

    /// Builds an RSA signing key from a PEM-encoded private key.
    #[inline]
    pub fn from_rsa_pem(pem: &[u8]) -> Result<Self, Error> {
        jsonwebtoken::EncodingKey::from_rsa_pem(pem)
            .map(Self)
            .map_err(Error::from_jwt_error)
    }

    /// Builds an ECDSA signing key from a PEM-encoded private key.
    #[inline]
    pub fn from_ec_pem(pem: &[u8]) -> Result<Self, Error> {
        jsonwebtoken::EncodingKey::from_ec_pem(pem)
            .map(Self)
            .map_err(Error::from_jwt_error)
    }

    /// Builds an EdDSA signing key from a PEM-encoded private key.
    #[inline]
    pub fn from_ed_pem(pem: &[u8]) -> Result<Self, Error> {
        jsonwebtoken::EncodingKey::from_ed_pem(pem)
            .map(Self)
            .map_err(Error::from_jwt_error)
    }

    /// Reads the env var `name` and builds an HMAC key from its bytes.
    ///
    /// Equivalent to `try_from_env(name).expect(...)`. Panics if the variable
    /// is missing or not valid UTF-8. Intended for startup configuration where
    /// failing fast is preferred.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::EncodingKey;
    ///
    /// let key = EncodingKey::from_env("JWT_SECRET");
    /// ```
    #[inline]
    pub fn from_env(name: &str) -> Self {
        Self::try_from_env(name).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Reads the env var `name` and builds an HMAC key from its bytes.
    ///
    /// Returns an error with a message that includes the variable name if the
    /// variable is missing or not valid UTF-8.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::EncodingKey;
    ///
    /// let key = EncodingKey::try_from_env("JWT_SECRET")?;
    /// # Ok::<(), volga::error::Error>(())
    /// ```
    #[inline]
    pub fn try_from_env(name: &str) -> Result<Self, Error> {
        let value = read_env_var(name)?;
        Ok(Self::from_secret(value.as_bytes()))
    }

    /// Reads the env var `name` and base64-decodes it into an HMAC key.
    ///
    /// Panics if the variable is missing, not UTF-8, or not valid base64.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::EncodingKey;
    ///
    /// let key = EncodingKey::from_env_base64("JWT_SECRET_B64");
    /// ```
    #[inline]
    pub fn from_env_base64(name: &str) -> Self {
        Self::try_from_env_base64(name).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Reads the env var `name` and base64-decodes it into an HMAC key.
    #[inline]
    pub fn try_from_env_base64(name: &str) -> Result<Self, Error> {
        let value = read_env_var(name)?;
        Self::from_base64_secret(&value)
    }

    /// Reads the file at `path` and builds an HMAC key from its raw bytes.
    ///
    /// Panics on I/O errors. Intended for startup configuration.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::EncodingKey;
    ///
    /// let key = EncodingKey::from_file("/etc/volga/jwt.key");
    /// ```
    #[inline]
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Self {
        Self::try_from_file(path).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Reads the file at `path` and builds an HMAC key from its raw bytes.
    #[inline]
    pub fn try_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Error> {
        let bytes = read_key_file(path.as_ref())?;
        Ok(Self::from_secret(&bytes))
    }

    /// Reads the PEM file at `path` and auto-detects the key format
    /// (RSA / EC / Ed) from the header line.
    ///
    /// Panics if the file cannot be read, the header is unrecognized, or
    /// the PEM body cannot be parsed by any candidate algorithm.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::EncodingKey;
    ///
    /// let key = EncodingKey::from_pem_file("/etc/volga/rs256.pem");
    /// ```
    #[inline]
    pub fn from_pem_file<P: AsRef<std::path::Path>>(path: P) -> Self {
        Self::try_from_pem_file(path).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Reads the PEM file at `path` and auto-detects the key format.
    pub fn try_from_pem_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Error> {
        let bytes = read_key_file(path.as_ref())?;
        match super::pem::detect(&bytes) {
            super::pem::PemKind::Rsa => Self::from_rsa_pem(&bytes),
            super::pem::PemKind::Ec => Self::from_ec_pem(&bytes),
            super::pem::PemKind::Ambiguous => Self::from_rsa_pem(&bytes)
                .or_else(|_| Self::from_ec_pem(&bytes))
                .or_else(|_| Self::from_ed_pem(&bytes)),
            super::pem::PemKind::Unknown => Err(Error::server_error(format!(
                "Unrecognized PEM header in {}; use from_rsa_pem / from_ec_pem / from_ed_pem explicitly",
                path.as_ref().display()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-bytes";
    // echo -n "test-secret-bytes" | base64 -> dGVzdC1zZWNyZXQtYnl0ZXM=
    const SECRET_B64: &str = "dGVzdC1zZWNyZXQtYnl0ZXM=";

    #[test]
    fn it_creates_from_secret() {
        let _key = EncodingKey::from_secret(SECRET);
    }

    #[test]
    fn it_creates_from_base64_secret() {
        let key = EncodingKey::from_base64_secret(SECRET_B64);
        assert!(key.is_ok());
    }

    #[test]
    fn it_rejects_invalid_base64_secret() {
        let key = EncodingKey::from_base64_secret("not valid base64!!!");
        assert!(key.is_err());
    }

    #[test]
    fn it_debugs_as_redacted() {
        let key = EncodingKey::from_secret(SECRET);
        assert_eq!(format!("{key:?}"), "EncodingKey([redacted])");
    }

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

    #[test]
    fn it_creates_from_rsa_pem() {
        let key = EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM);
        assert!(key.is_ok());
    }

    #[test]
    fn it_rejects_malformed_rsa_pem() {
        let key = EncodingKey::from_rsa_pem(b"not a pem");
        assert!(key.is_err());
    }

    #[test]
    fn it_loads_from_env_var() {
        // CARGO_PKG_NAME is always set by cargo during tests; use as known-present var.
        let key = EncodingKey::try_from_env("CARGO_PKG_NAME");
        assert!(key.is_ok(), "got: {key:?}");
    }

    #[test]
    fn it_fails_when_env_var_missing() {
        let key = EncodingKey::try_from_env("DEFINITELY_NOT_SET_VAR_XYZABC_123456");
        assert!(key.is_err());
        let err_msg = key.unwrap_err().to_string();
        assert!(
            err_msg.contains("DEFINITELY_NOT_SET_VAR_XYZABC_123456"),
            "error should mention var name, got: {err_msg}"
        );
    }

    #[test]
    #[should_panic(expected = "NOT_SET_PANIC_ENCODING_SECRET_XYZABC")]
    fn it_panics_when_from_env_var_missing() {
        let _ = EncodingKey::from_env("NOT_SET_PANIC_ENCODING_SECRET_XYZABC");
    }

    #[test]
    fn it_fails_env_base64_when_var_invalid_base64() {
        // CARGO_PKG_NAME is always set by cargo during tests and is not valid base64.
        let key = EncodingKey::try_from_env_base64("CARGO_PKG_NAME");
        assert!(key.is_err());
    }

    #[test]
    fn it_fails_env_base64_when_var_missing() {
        let key = EncodingKey::try_from_env_base64("VOLGA_TEST_ENV_B64_MISSING_XYZ");
        assert!(key.is_err());
    }

    #[test]
    fn it_loads_from_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("volga-test-encoding-{}.key", std::process::id()));
        std::fs::write(&path, SECRET).unwrap();
        let key = EncodingKey::try_from_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(key.is_ok());
    }

    #[test]
    fn it_fails_when_file_missing() {
        let path = std::path::Path::new("/nonexistent/volga/test/key.txt");
        let key = EncodingKey::try_from_file(path);
        assert!(key.is_err());
    }

    #[test]
    fn it_loads_rsa_pem_file_with_autodetect() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "volga-test-encoding-rsa-{}.pem",
            std::process::id()
        ));
        std::fs::write(&path, RSA_PRIVATE_PEM).unwrap();
        let key = EncodingKey::try_from_pem_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(key.is_ok(), "got: {key:?}");
    }

    #[test]
    fn it_fails_when_pem_file_missing() {
        let path = std::path::Path::new("/nonexistent/volga/test/key.pem");
        let key = EncodingKey::try_from_pem_file(path);
        assert!(key.is_err());
    }

    #[test]
    fn it_fails_when_pem_header_unknown() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "volga-test-encoding-unknown-{}.pem",
            std::process::id()
        ));
        std::fs::write(
            &path,
            b"-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let key = EncodingKey::try_from_pem_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(key.is_err());
        assert!(key.unwrap_err().to_string().to_lowercase().contains("pem"));
    }
}

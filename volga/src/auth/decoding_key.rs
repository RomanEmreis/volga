//! Owned wrapper around `jsonwebtoken::DecodingKey`.

use crate::auth::encoding_key::{read_env_var, read_key_file};
use crate::error::Error;

/// A key used to verify JWTs.
///
/// Wraps an internal verification key. Use one of the `from_*` or `try_from_*`
/// constructors to build an instance, then pass it to
/// [`BearerAuthConfig::set_decoding_key`](super::bearer::BearerAuthConfig::set_decoding_key).
pub struct DecodingKey(pub(crate) jsonwebtoken::DecodingKey);

impl std::fmt::Debug for DecodingKey {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DecodingKey([redacted])")
    }
}

impl DecodingKey {
    /// Builds an HMAC key from a raw byte slice.
    #[inline]
    pub fn from_secret(secret: &[u8]) -> Self {
        Self(jsonwebtoken::DecodingKey::from_secret(secret))
    }

    /// Builds an HMAC key by base64-decoding a string.
    #[inline]
    pub fn from_base64_secret(secret: &str) -> Result<Self, Error> {
        jsonwebtoken::DecodingKey::from_base64_secret(secret)
            .map(Self)
            .map_err(Error::from)
    }

    /// Builds an RSA verification key from a PEM-encoded public key.
    #[inline]
    pub fn from_rsa_pem(pem: &[u8]) -> Result<Self, Error> {
        jsonwebtoken::DecodingKey::from_rsa_pem(pem)
            .map(Self)
            .map_err(Error::from)
    }

    /// Builds an ECDSA verification key from a PEM-encoded public key.
    #[inline]
    pub fn from_ec_pem(pem: &[u8]) -> Result<Self, Error> {
        jsonwebtoken::DecodingKey::from_ec_pem(pem)
            .map(Self)
            .map_err(Error::from)
    }

    /// Builds an EdDSA verification key from a PEM-encoded public key.
    #[inline]
    pub fn from_ed_pem(pem: &[u8]) -> Result<Self, Error> {
        jsonwebtoken::DecodingKey::from_ed_pem(pem)
            .map(Self)
            .map_err(Error::from)
    }

    /// Reads the env var `name` and builds an HMAC key from its bytes.
    ///
    /// Panics on missing/invalid env var. Intended for startup configuration.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::DecodingKey;
    ///
    /// let key = DecodingKey::from_env("JWT_SECRET");
    /// ```
    #[inline]
    pub fn from_env(name: &str) -> Self {
        Self::try_from_env(name).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Reads the env var `name` and builds an HMAC key from its bytes.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::DecodingKey;
    ///
    /// let key = DecodingKey::try_from_env("JWT_SECRET")?;
    /// # Ok::<(), volga::error::Error>(())
    /// ```
    #[inline]
    pub fn try_from_env(name: &str) -> Result<Self, Error> {
        let value = read_env_var(name)?;
        Ok(Self::from_secret(value.as_bytes()))
    }

    /// Reads the env var `name` and base64-decodes it into an HMAC key.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::DecodingKey;
    ///
    /// let key = DecodingKey::from_env_base64("JWT_SECRET_B64");
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
    /// # Example
    /// ```no_run
    /// use volga::auth::DecodingKey;
    ///
    /// let key = DecodingKey::from_file("/etc/volga/jwt.key");
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
    /// the PEM body cannot be parsed.
    ///
    /// # Example
    /// ```no_run
    /// use volga::auth::DecodingKey;
    ///
    /// let key = DecodingKey::from_pem_file("/etc/volga/rs256.pub");
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
    const SECRET_B64: &str = "dGVzdC1zZWNyZXQtYnl0ZXM=";

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

    #[test]
    fn it_creates_from_secret() {
        let _ = DecodingKey::from_secret(SECRET);
    }

    #[test]
    fn it_creates_from_base64_secret() {
        assert!(DecodingKey::from_base64_secret(SECRET_B64).is_ok());
    }

    #[test]
    fn it_rejects_invalid_base64_secret() {
        let key = DecodingKey::from_base64_secret("not valid base64!!!");
        assert!(key.is_err());
    }

    #[test]
    fn it_creates_from_rsa_pem() {
        let key = DecodingKey::from_rsa_pem(RSA_PUBLIC_PEM);
        assert!(key.is_ok());
    }

    #[test]
    fn it_rejects_malformed_rsa_pem() {
        let key = DecodingKey::from_rsa_pem(b"not a pem");
        assert!(key.is_err());
    }

    #[test]
    fn it_rejects_malformed_ec_pem() {
        let key = DecodingKey::from_ec_pem(b"not a pem");
        assert!(key.is_err());
    }

    #[test]
    fn it_rejects_malformed_ed_pem() {
        let key = DecodingKey::from_ed_pem(b"not a pem");
        assert!(key.is_err());
    }

    #[test]
    fn it_loads_from_env_var() {
        // CARGO_PKG_NAME is always set by cargo during tests.
        let key = DecodingKey::try_from_env("CARGO_PKG_NAME");
        assert!(key.is_ok());
    }

    #[test]
    fn it_fails_when_env_missing() {
        let key = DecodingKey::try_from_env("VOLGA_TEST_DECODING_KEY_NOT_SET_XYZ");
        assert!(key.is_err());
    }

    #[test]
    #[should_panic(expected = "NOT_SET_PANIC_DECODING_SECRET_XYZABC")]
    fn it_panics_when_from_env_var_missing() {
        let _ = DecodingKey::from_env("NOT_SET_PANIC_DECODING_SECRET_XYZABC");
    }

    #[test]
    fn it_fails_env_base64_when_var_invalid_base64() {
        // CARGO_PKG_NAME is always set by cargo during tests and is not valid base64.
        let key = DecodingKey::try_from_env_base64("CARGO_PKG_NAME");
        assert!(key.is_err());
    }

    #[test]
    fn it_fails_env_base64_when_var_missing() {
        let key = DecodingKey::try_from_env_base64("VOLGA_TEST_DECODING_ENV_B64_MISSING_XYZ");
        assert!(key.is_err());
    }

    #[test]
    fn it_loads_from_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("volga-test-decoding-{}.key", std::process::id()));
        std::fs::write(&path, SECRET).unwrap();
        let key = DecodingKey::try_from_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(key.is_ok());
    }

    #[test]
    fn it_fails_when_file_missing() {
        let path = std::path::Path::new("/nonexistent/volga/test/decoding-key.txt");
        let key = DecodingKey::try_from_file(path);
        assert!(key.is_err());
    }

    #[test]
    fn it_loads_rsa_pem_file_with_autodetect() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "volga-test-decoding-rsa-{}.pem",
            std::process::id()
        ));
        std::fs::write(&path, RSA_PUBLIC_PEM).unwrap();
        let key = DecodingKey::try_from_pem_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(key.is_ok(), "got: {key:?}");
    }

    #[test]
    fn it_fails_when_pem_file_missing() {
        let path = std::path::Path::new("/nonexistent/volga/test/decoding-key.pem");
        let key = DecodingKey::try_from_pem_file(path);
        assert!(key.is_err());
    }

    #[test]
    fn it_fails_when_pem_header_unknown() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "volga-test-decoding-unknown-{}.pem",
            std::process::id()
        ));
        std::fs::write(
            &path,
            b"-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let key = DecodingKey::try_from_pem_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(key.is_err());
        assert!(key.unwrap_err().to_string().to_lowercase().contains("pem"));
    }

    #[test]
    fn it_debugs_as_redacted() {
        let key = DecodingKey::from_secret(SECRET);
        assert_eq!(format!("{key:?}"), "DecodingKey([redacted])");
    }
}

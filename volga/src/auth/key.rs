//! Shared loading surface of the JWT key wrappers.
//!
//! [`EncodingKey`](super::EncodingKey) and [`DecodingKey`](super::DecodingKey)
//! are newtypes over the two halves of the same key material, and every way
//! of getting one - a raw secret, a base64 secret, a PEM blob, an env var, a
//! file - is the same code with a different inner type. [`jwt_key`] generates
//! that surface for both, so the two stay in step: a loader added or a
//! behaviour fixed lands on the encoding and decoding sides at once.

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

/// Generates a JWT key newtype over `jsonwebtoken::$inner` together with the
/// whole loading surface.
///
/// `$kind` ("signing" / "verification") and `$half` ("private" / "public")
/// only shape the documentation; everything else is identical between the
/// two keys.
macro_rules! jwt_key {
    (
        $(#[$meta:meta])*
        $name:ident, $inner:ident, kind = $kind:literal, half = $half:literal
    ) => {
        $(#[$meta])*
        pub struct $name(pub(crate) jsonwebtoken::$inner);

        impl std::fmt::Debug for $name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                // key material is a credential - never expose it
                f.write_str(concat!(stringify!($name), "([redacted])"))
            }
        }

        impl $name {
            #[doc = concat!("Builds an HMAC key from a raw byte slice.")]
            ///
            /// Use with `HS256`, `HS384`, or `HS512`.
            #[inline]
            pub fn from_secret(secret: &[u8]) -> Self {
                Self(jsonwebtoken::$inner::from_secret(secret))
            }

            /// Builds an HMAC key by base64-decoding a string.
            ///
            /// Returns an error if the input is not valid base64.
            #[inline]
            pub fn from_base64_secret(secret: &str) -> Result<Self, $crate::error::Error> {
                jsonwebtoken::$inner::from_base64_secret(secret)
                    .map(Self)
                    .map_err($crate::error::Error::from_jwt_error)
            }

            #[doc = concat!(
                "Builds an RSA ", $kind, " key from a PEM-encoded ", $half, " key."
            )]
            #[inline]
            pub fn from_rsa_pem(pem: &[u8]) -> Result<Self, $crate::error::Error> {
                jsonwebtoken::$inner::from_rsa_pem(pem)
                    .map(Self)
                    .map_err($crate::error::Error::from_jwt_error)
            }

            #[doc = concat!(
                "Builds an ECDSA ", $kind, " key from a PEM-encoded ", $half, " key."
            )]
            #[inline]
            pub fn from_ec_pem(pem: &[u8]) -> Result<Self, $crate::error::Error> {
                jsonwebtoken::$inner::from_ec_pem(pem)
                    .map(Self)
                    .map_err($crate::error::Error::from_jwt_error)
            }

            #[doc = concat!(
                "Builds an EdDSA ", $kind, " key from a PEM-encoded ", $half, " key."
            )]
            #[inline]
            pub fn from_ed_pem(pem: &[u8]) -> Result<Self, $crate::error::Error> {
                jsonwebtoken::$inner::from_ed_pem(pem)
                    .map(Self)
                    .map_err($crate::error::Error::from_jwt_error)
            }

            /// Reads the env var `name` and builds an HMAC key from its bytes.
            ///
            /// Equivalent to `try_from_env(name).expect(...)`. Panics if the
            /// variable is missing or not valid UTF-8. Intended for startup
            /// configuration where failing fast is preferred.
            ///
            /// # Example
            /// ```no_run
            #[doc = concat!("use volga::auth::", stringify!($name), ";")]
            ///
            #[doc = concat!("let key = ", stringify!($name), "::from_env(\"JWT_SECRET\");")]
            /// ```
            #[inline]
            pub fn from_env(name: &str) -> Self {
                Self::try_from_env(name).unwrap_or_else(|e| panic!("{e}"))
            }

            /// Reads the env var `name` and builds an HMAC key from its bytes.
            ///
            /// Returns an error with a message that includes the variable
            /// name if the variable is missing or not valid UTF-8.
            ///
            /// # Example
            /// ```no_run
            #[doc = concat!("use volga::auth::", stringify!($name), ";")]
            ///
            #[doc = concat!(
                "let key = ", stringify!($name), "::try_from_env(\"JWT_SECRET\")?;"
            )]
            /// # Ok::<(), volga::error::Error>(())
            /// ```
            #[inline]
            pub fn try_from_env(name: &str) -> Result<Self, $crate::error::Error> {
                let value = $crate::auth::key::read_env_var(name)?;
                Ok(Self::from_secret(value.as_bytes()))
            }

            /// Reads the env var `name` and base64-decodes it into an HMAC key.
            ///
            /// Panics if the variable is missing, not UTF-8, or not valid
            /// base64.
            ///
            /// # Example
            /// ```no_run
            #[doc = concat!("use volga::auth::", stringify!($name), ";")]
            ///
            #[doc = concat!(
                "let key = ", stringify!($name), "::from_env_base64(\"JWT_SECRET_B64\");"
            )]
            /// ```
            #[inline]
            pub fn from_env_base64(name: &str) -> Self {
                Self::try_from_env_base64(name).unwrap_or_else(|e| panic!("{e}"))
            }

            /// Reads the env var `name` and base64-decodes it into an HMAC key.
            #[inline]
            pub fn try_from_env_base64(name: &str) -> Result<Self, $crate::error::Error> {
                let value = $crate::auth::key::read_env_var(name)?;
                Self::from_base64_secret(&value)
            }

            /// Reads the file at `path` and builds an HMAC key from its raw
            /// bytes.
            ///
            /// Panics on I/O errors. Intended for startup configuration.
            ///
            /// # Example
            /// ```no_run
            #[doc = concat!("use volga::auth::", stringify!($name), ";")]
            ///
            #[doc = concat!(
                "let key = ", stringify!($name), "::from_file(\"/etc/volga/jwt.key\");"
            )]
            /// ```
            #[inline]
            pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Self {
                Self::try_from_file(path).unwrap_or_else(|e| panic!("{e}"))
            }

            /// Reads the file at `path` and builds an HMAC key from its raw
            /// bytes.
            #[inline]
            pub fn try_from_file<P: AsRef<std::path::Path>>(
                path: P,
            ) -> Result<Self, $crate::error::Error> {
                let bytes = $crate::auth::key::read_key_file(path.as_ref())?;
                Ok(Self::from_secret(&bytes))
            }

            /// Reads the PEM file at `path` and auto-detects the key format
            /// (RSA / EC / Ed) from the header line.
            ///
            /// Panics if the file cannot be read, the header is
            /// unrecognized, or the PEM body cannot be parsed by any
            /// candidate algorithm.
            ///
            /// # Example
            /// ```no_run
            #[doc = concat!("use volga::auth::", stringify!($name), ";")]
            ///
            #[doc = concat!(
                "let key = ", stringify!($name), "::from_pem_file(\"/etc/volga/rs256.pem\");"
            )]
            /// ```
            #[inline]
            pub fn from_pem_file<P: AsRef<std::path::Path>>(path: P) -> Self {
                Self::try_from_pem_file(path).unwrap_or_else(|e| panic!("{e}"))
            }

            /// Reads the PEM file at `path` and auto-detects the key format.
            pub fn try_from_pem_file<P: AsRef<std::path::Path>>(
                path: P,
            ) -> Result<Self, $crate::error::Error> {
                let bytes = $crate::auth::key::read_key_file(path.as_ref())?;
                match volga_oauth_core::pem::detect(&bytes) {
                    volga_oauth_core::pem::PemKind::Rsa => Self::from_rsa_pem(&bytes),
                    volga_oauth_core::pem::PemKind::Ec => Self::from_ec_pem(&bytes),
                    // the unqualified header names no algorithm: try each
                    // candidate rather than guessing from the body
                    volga_oauth_core::pem::PemKind::Ambiguous => Self::from_rsa_pem(&bytes)
                        .or_else(|_| Self::from_ec_pem(&bytes))
                        .or_else(|_| Self::from_ed_pem(&bytes)),
                    volga_oauth_core::pem::PemKind::Unknown => {
                        Err($crate::error::Error::server_error(format!(
                            "Unrecognized PEM header in {}; use from_rsa_pem / from_ec_pem / \
                             from_ed_pem explicitly",
                            path.as_ref().display()
                        )))
                    }
                }
            }
        }
    };
}

pub(crate) use jwt_key;

/// The shared behaviour, asserted once per key type by the two modules
/// that invoke [`jwt_key`].
#[cfg(test)]
macro_rules! jwt_key_tests {
    ($name:ident, pem = $pem:ident, slug = $slug:literal) => {
        const SECRET: &[u8] = b"test-secret-bytes";
        // echo -n "test-secret-bytes" | base64 -> dGVzdC1zZWNyZXQtYnl0ZXM=
        const SECRET_B64: &str = "dGVzdC1zZWNyZXQtYnl0ZXM=";

        /// A unique temp path for this test process and case.
        fn temp_path(case: &str, extension: &str) -> std::path::PathBuf {
            std::env::temp_dir().join(format!(
                "volga-test-{}-{case}-{}.{extension}",
                $slug,
                std::process::id()
            ))
        }

        #[test]
        fn it_creates_from_secrets() {
            let _ = $name::from_secret(SECRET);
            assert!($name::from_base64_secret(SECRET_B64).is_ok());
            assert!($name::from_base64_secret("not valid base64!!!").is_err());
        }

        #[test]
        fn it_creates_from_pem() {
            assert!($name::from_rsa_pem($pem).is_ok());
            assert!($name::from_rsa_pem(b"not a pem").is_err());
            assert!($name::from_ec_pem(b"not a pem").is_err());
            assert!($name::from_ed_pem(b"not a pem").is_err());
        }

        #[test]
        fn it_loads_from_env_var() {
            // CARGO_PKG_NAME is always set by cargo during tests; use as
            // known-present var
            assert!($name::try_from_env("CARGO_PKG_NAME").is_ok());

            let err = $name::try_from_env("VOLGA_TEST_KEY_NOT_SET_XYZ")
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("VOLGA_TEST_KEY_NOT_SET_XYZ"),
                "error should mention var name, got: {err}"
            );

            // ...and CARGO_PKG_NAME is not valid base64
            assert!($name::try_from_env_base64("CARGO_PKG_NAME").is_err());
            assert!($name::try_from_env_base64("VOLGA_TEST_KEY_B64_MISSING_XYZ").is_err());
        }

        #[test]
        #[should_panic(expected = "VOLGA_TEST_KEY_PANIC_XYZABC")]
        fn it_panics_when_env_var_is_missing() {
            let _ = $name::from_env("VOLGA_TEST_KEY_PANIC_XYZABC");
        }

        #[test]
        fn it_loads_from_file() {
            let path = temp_path("raw", "key");
            std::fs::write(&path, SECRET).unwrap();
            let key = $name::try_from_file(&path);
            let _ = std::fs::remove_file(&path);
            assert!(key.is_ok());

            assert!($name::try_from_file("/nonexistent/volga/test/key.txt").is_err());
        }

        #[test]
        fn it_loads_a_pem_file_with_autodetect() {
            let path = temp_path("pem", "pem");
            std::fs::write(&path, $pem).unwrap();
            let key = $name::try_from_pem_file(&path);
            let _ = std::fs::remove_file(&path);
            assert!(key.is_ok(), "got: {key:?}");

            assert!($name::try_from_pem_file("/nonexistent/volga/test/key.pem").is_err());
        }

        #[test]
        fn it_fails_when_the_pem_header_is_unknown() {
            let path = temp_path("unknown", "pem");
            std::fs::write(
                &path,
                b"-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----\n",
            )
            .unwrap();
            let key = $name::try_from_pem_file(&path);
            let _ = std::fs::remove_file(&path);
            let err = key.unwrap_err().to_string().to_lowercase();
            assert!(err.contains("pem"), "got: {err}");
        }

        #[test]
        fn it_debugs_as_redacted() {
            let key = $name::from_secret(SECRET);
            assert_eq!(
                format!("{key:?}"),
                concat!(stringify!($name), "([redacted])")
            );
        }
    };
}

#[cfg(test)]
pub(crate) use jwt_key_tests;

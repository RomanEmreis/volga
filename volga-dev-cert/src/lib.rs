//! A Rust library for generating self-signed TLS certificates for local development.

use rcgen::{generate_simple_self_signed, CertifiedKey};
use std::{fs, path::PathBuf, io::{Error, Result, Write}};

/// Defaut name of folder with TLS certificates
pub const DEFAULT_CERT_FOLDER: &str = "cert";

/// Default name of development certificate file
pub const DEFAULT_CERT_FILE_NAME: &str = "dev-cert.pem";

/// Default name of signing key file
pub const DEFAULT_KEY_FILE_NAME: &str = "dev-key.pem";

/// Default certificate names
#[cfg(target_os = "windows")]
pub const DEV_CERT_NAMES: &[&str] = &["localhost"];
/// Default certificate names
#[cfg(not(target_os = "windows"))]
pub const DEV_CERT_NAMES: &[&str] = &["localhost", "0.0.0.0"];

/// Generates self-signed certificate and saves them into `./cert` folder
#[inline]
pub fn generate(names: impl Into<Vec<String>>) -> Result<()> {
    let CertifiedKey { cert, signing_key } = generate_simple_self_signed(names)
        .map_err(|err| Error::other(format!("{:?}", err)))?;
    fs::create_dir_all(DEFAULT_CERT_FOLDER)?;
    fs::write(get_cert_path(), cert.pem())?;
    fs::write(get_signing_key_path(), signing_key.serialize_pem())?;
    Ok(())
}

/// Sends the message to the `stdio` that asks whether to create dev TLS certificate of not
#[inline]
pub fn ask_generate() -> Result<bool> {
    print!("Dev certificate not found. Generate new one? (y/n): ");
    std::io::stdout().flush()?;

    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("y"))
}

/// Checks whether dev certificate exists
#[inline]
pub fn dev_cert_exists() -> bool {
    get_cert_path().exists() && 
    get_signing_key_path().exists()
}

/// Returns default path to the development TLS certificate .pem file
#[inline]
pub fn get_cert_path() -> PathBuf {
    PathBuf::from(DEFAULT_CERT_FOLDER)
        .join(DEFAULT_CERT_FILE_NAME)
}

/// Returns default path to the signin key .pem file 
#[inline]
pub fn get_signing_key_path() -> PathBuf {
    PathBuf::from(DEFAULT_CERT_FOLDER)
        .join(DEFAULT_KEY_FILE_NAME)
}

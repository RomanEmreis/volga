//! A Rust library for generating self-signed TLS certificates for local development.

use rcgen::{generate_simple_self_signed, CertifiedKey};
use std::{fs, path::PathBuf, io::{Error, Result, Write}};

/// Default name of a folder with TLS certificates
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
    let names = names.into();

    // Validate that names are not empty
    if names.is_empty() {
        return Err(Error::other("Certificate names cannot be empty"));
    }
    
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

/// Checks whether a dev certificate exists
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use serial_test::serial;

    // Helper to clean up before and after test
    fn cleanup() {
        let _ = fs::remove_dir_all(DEFAULT_CERT_FOLDER);
    }

    #[test]
    fn it_defines_default_cert_folder_constant() {
        assert_eq!(DEFAULT_CERT_FOLDER, "cert");
    }

    #[test]
    fn it_defines_default_cert_file_name_constant() {
        assert_eq!(DEFAULT_CERT_FILE_NAME, "dev-cert.pem");
    }

    #[test]
    fn it_defines_default_key_file_name_constant() {
        assert_eq!(DEFAULT_KEY_FILE_NAME, "dev-key.pem");
    }

    #[test]
    fn it_defines_dev_cert_names_for_localhost() {
        assert!(DEV_CERT_NAMES.contains(&"localhost"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn it_includes_zero_address_in_dev_cert_names_on_unix() {
        assert!(DEV_CERT_NAMES.contains(&"0.0.0.0"));
        assert_eq!(DEV_CERT_NAMES.len(), 2);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn it_excludes_zero_address_in_dev_cert_names_on_windows() {
        assert!(!DEV_CERT_NAMES.contains(&"0.0.0.0"));
        assert_eq!(DEV_CERT_NAMES.len(), 1);
    }

    #[test]
    fn it_constructs_cert_path_correctly() {
        let path = get_cert_path();

        assert_eq!(path.file_name().unwrap(), DEFAULT_CERT_FILE_NAME);
        assert!(path.to_string_lossy().contains(DEFAULT_CERT_FOLDER));
    }

    #[test]
    fn it_constructs_signing_key_path_correctly() {
        let path = get_signing_key_path();

        assert_eq!(path.file_name().unwrap(), DEFAULT_KEY_FILE_NAME);
        assert!(path.to_string_lossy().contains(DEFAULT_CERT_FOLDER));
    }

    #[test]
    #[serial]
    fn it_returns_false_when_dev_cert_does_not_exist() {
        cleanup();

        assert!(!dev_cert_exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_returns_false_when_only_cert_file_exists() {
        cleanup();

        fs::create_dir_all(DEFAULT_CERT_FOLDER).unwrap();
        fs::write(get_cert_path(), "dummy cert").unwrap();

        assert!(!dev_cert_exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_returns_false_when_only_key_file_exists() {
        cleanup();

        fs::create_dir_all(DEFAULT_CERT_FOLDER).unwrap();
        fs::write(get_signing_key_path(), "dummy key").unwrap();

        assert!(!dev_cert_exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_returns_true_when_both_cert_files_exist() {
        cleanup();

        fs::create_dir_all(DEFAULT_CERT_FOLDER).unwrap();
        fs::write(get_cert_path(), "dummy cert").unwrap();
        fs::write(get_signing_key_path(), "dummy key").unwrap();

        assert!(dev_cert_exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_generates_certificate_with_single_name() {
        cleanup();

        let result = generate(vec!["test.local".to_string()]);

        assert!(result.is_ok());
        assert!(get_cert_path().exists());
        assert!(get_signing_key_path().exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_generates_certificate_with_multiple_names() {
        cleanup();

        let names = vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "test.local".to_string(),
        ];

        let result = generate(names);

        assert!(result.is_ok());
        assert!(get_cert_path().exists());
        assert!(get_signing_key_path().exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_creates_cert_folder_if_not_exists() {
        cleanup();

        let result = generate(vec!["localhost".to_string()]);

        assert!(result.is_ok());
        assert!(Path::new(DEFAULT_CERT_FOLDER).exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_writes_pem_formatted_certificate() {
        cleanup();

        generate(vec!["localhost".to_string()]).unwrap();

        let cert_content = fs::read_to_string(get_cert_path()).unwrap();

        assert!(cert_content.contains("-----BEGIN CERTIFICATE-----"));
        assert!(cert_content.contains("-----END CERTIFICATE-----"));

        cleanup();
    }

    #[test]
    #[serial]
    fn it_writes_pem_formatted_signing_key() {
        cleanup();

        generate(vec!["localhost".to_string()]).unwrap();

        let key_content = fs::read_to_string(get_signing_key_path()).unwrap();

        assert!(key_content.contains("-----BEGIN PRIVATE KEY-----") ||
            key_content.contains("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(key_content.contains("-----END PRIVATE KEY-----") ||
            key_content.contains("-----END RSA PRIVATE KEY-----"));

        cleanup();
    }

    #[test]
    #[serial]
    fn it_overwrites_existing_certificates() {
        cleanup();

        generate(vec!["first.local".to_string()]).unwrap();
        let first_cert = fs::read_to_string(get_cert_path()).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        generate(vec!["second.local".to_string()]).unwrap();
        let second_cert = fs::read_to_string(get_cert_path()).unwrap();

        assert_ne!(first_cert, second_cert);

        cleanup();
    }

    #[test]
    #[serial]
    fn it_generates_valid_certificate_structure() {
        cleanup();

        let result = generate(vec!["localhost".to_string()]);

        assert!(result.is_ok());

        let cert_content = fs::read_to_string(get_cert_path()).unwrap();
        let key_content = fs::read_to_string(get_signing_key_path()).unwrap();

        assert!(!cert_content.is_empty());
        assert!(!key_content.is_empty());

        assert!(cert_content.lines().count() > 2);
        assert!(key_content.lines().count() > 2);

        cleanup();
    }

    #[test]
    #[serial]
    fn it_handles_empty_names_vector() {
        cleanup();

        let result = generate(Vec::<String>::new());
        assert!(result.is_err());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_generates_certificate_with_default_names() {
        cleanup();

        let names: Vec<String> = DEV_CERT_NAMES.iter().map(|s| s.to_string()).collect();
        let result = generate(names);

        assert!(result.is_ok());
        assert!(dev_cert_exists());

        cleanup();
    }

    #[test]
    fn it_constructs_paths_with_correct_separators() {
        let cert_path = get_cert_path();
        let key_path = get_signing_key_path();

        assert!(cert_path.is_relative());
        assert!(key_path.is_relative());

        let cert_components: Vec<_> = cert_path.components().collect();
        let key_components: Vec<_> = key_path.components().collect();

        assert_eq!(cert_components.len(), 2);
        assert_eq!(key_components.len(), 2);
    }

    #[test]
    #[serial]
    fn it_generates_different_certificates_for_different_names() {
        cleanup();

        generate(vec!["name1.local".to_string()]).unwrap();
        let cert1 = fs::read(get_cert_path()).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        generate(vec!["name2.local".to_string()]).unwrap();
        let cert2 = fs::read(get_cert_path()).unwrap();

        assert_ne!(cert1, cert2);

        cleanup();
    }

    #[test]
    #[serial]
    fn it_handles_special_characters_in_names() {
        cleanup();

        let names = vec![
            "test-app.local".to_string(),
            "my_service.dev".to_string(),
        ];

        let result = generate(names);

        assert!(result.is_ok());
        assert!(dev_cert_exists());

        cleanup();
    }

    #[test]
    #[serial]
    fn it_verifies_cert_folder_is_created_before_files() {
        cleanup();

        assert!(!Path::new(DEFAULT_CERT_FOLDER).exists());

        generate(vec!["localhost".to_string()]).unwrap();

        assert!(Path::new(DEFAULT_CERT_FOLDER).exists());
        assert!(Path::new(DEFAULT_CERT_FOLDER).is_dir());

        cleanup();
    }
}
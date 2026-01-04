//! File system utilities
//!
//! This module provides helpers for working with temporary files,
//! such as [`TempFile`], which is useful when testing file uploads
//! or filesystem-backed APIs.

use tokio::{fs::File as TokioFile, io::AsyncWriteExt};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A temporary file utility intended for use in integration tests.
///
/// `TempFile` creates a file inside a uniquely owned temporary directory.
/// Both the file and its parent directory are automatically removed
/// when the `TempFile` instance is dropped.
///
/// This type is useful for testing endpoints that work with file uploads,
/// downloads, or filesystem-backed operations without touching the real
/// filesystem.
///
/// # Lifecycle
///
/// - A new temporary directory is created on construction.
/// - The file is created inside that directory.
/// - When `TempFile` is dropped, the directory and all its contents
///   are removed automatically.
///
/// # Notes
///
/// - Each `TempFile` instance is fully isolated.
/// - The file path is stable for the lifetime of the instance.
/// - Cleanup is deterministic and requires no manual action.
///
/// # Example
///
/// ```no_run
/// use volga::test::TempFile;
///
/// #[tokio::test]
/// async fn upload_file() {
///     let file = TempFile::new("hello world").await;
///
///     assert!(file.path.exists());
///     assert_eq!(file.file_name().ends_with(".txt"), true);
///
///     // file and directory are removed when dropped
/// }
/// ```
#[derive(Debug)]
pub struct TempFile {
    /// Represents the path to the file inside the temporary directory.
    pub path: PathBuf,
    dir: tempfile::TempDir,
}

impl TempFile {
    /// Creates a new temporary file with a randomly generated name and
    /// writes the provided string content into it.
    ///
    /// The file name is generated using a UUID and has a `.txt` extension.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use volga::test::TempFile;
    /// # async fn docs() {
    /// let file = TempFile::new("test content").await;
    /// # }
    /// ```
    pub async fn new(content: &str) -> Self {
        let random_name = format!("{}.txt", Uuid::new_v4());
        Self::with_name(random_name.as_str(), content).await
    }

    /// Creates an empty temporary file with a randomly generated name.
    ///
    /// The file is not created on disk immediately, but the path is reserved
    /// inside a temporary directory owned by this instance.
    ///
    /// This can be useful when the file is expected to be created or written
    /// by the system under test.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use volga::test::TempFile;
    /// 
    /// let file = TempFile::empty();
    ///
    /// assert!(!file.path.exists());
    /// ```
    pub fn empty() -> Self {
        let dir = tempfile::TempDir::new()
            .expect("Failed to create temp dir");

        let random_name = format!("{}.txt", Uuid::new_v4());
        let path = dir.path().join(random_name);
        
        Self { dir, path }       
    }

    /// Creates a new temporary file with the specified file name and
    /// writes the provided string content into it.
    ///
    /// The file is created inside a unique temporary directory.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use volga::test::TempFile;
    /// # async fn docs() {
    /// let file = TempFile::with_name("data.json", "{}").await;
    /// 
    /// assert_eq!(file.file_name(), "data.json");
    /// # }
    /// ```
    pub async fn with_name(name: &str, content: &str) -> Self {
        let dir = tempfile::TempDir::new()
            .expect("Failed to create temp dir");
        
        let path = dir.path().join(name);

        let mut file = TokioFile::create(&path)
            .await
            .expect("Failed to create file");
        
        file.write_all(content.as_bytes())
            .await
            .expect("Failed to write");
        
        file
            .flush()
            .await
            .expect("Failed to flush");
        
        drop(file);

        Self { dir, path }
    }

    /// Creates a new temporary file with the specified file name and
    /// writes the provided raw bytes into it.
    ///
    /// This is useful for working with binary data such as images,
    /// archives, or arbitrary payloads.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use volga::test::TempFile;
    /// # async fn docs() {
    /// let bytes = &[0xde, 0xad, 0xbe, 0xef];
    /// let file = TempFile::from_bytes("data.bin", bytes).await;
    /// # }
    /// ```
    pub async fn from_bytes(name: &str, bytes: &[u8]) -> Self {
        let dir = tempfile::TempDir::new()
            .expect("Failed to create temp dir");
        
        let path = dir.path().join(name);

        let mut file = TokioFile::create(&path)
            .await
            .expect("Failed to create file");
        
        file
            .write_all(bytes)
            .await
            .expect("Failed to write");
        
        file
            .flush()
            .await
            .expect("Failed to flush");
        
        drop(file);

        Self { dir, path }
    }

    /// Returns the file name portion of the temporary file path.
    ///
    /// # Panics
    ///
    /// Panics if the file name is not valid UTF-8. This should not happen
    /// for files created by `TempFile`.
    pub fn file_name(&self) -> &str {
        self.path.file_name()
            .and_then(|n| n.to_str())
            .expect("Invalid file name")
    }

    /// Returns the path to the temporary directory containing the file.
    ///
    /// This can be useful when the system under test expects a directory
    /// path rather than a file path.
    pub fn dir_path(&self) -> &Path {
        self.dir.path()
    }
}

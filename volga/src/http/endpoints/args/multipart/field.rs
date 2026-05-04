//! Multipart field — represents a single part of an incoming multipart stream.

use bytes::Bytes;
use std::{
    ops::{Deref, DerefMut},
    path::Path,
};
use tokio::io::{AsyncWriteExt, BufWriter};

use super::error::MultipartError;
use crate::error::Error;

/// Represents a single field in a multipart stream
///
///> See also [`multer::Field`]
#[derive(Debug)]
pub struct Field(pub(super) multer::Field<'static>);

impl Field {
    /// Tries to read a file name, if it's not present tries to read a field name, otherwise returns [`Error`]
    #[inline]
    pub fn try_get_file_name(&self) -> Result<&str, Error> {
        self.0
            .file_name()
            .or(self.name())
            .ok_or(MultipartError::missing_file_name())
    }

    /// Get the full field data as text.
    ///
    ///> See also [`multer::Field::text`]
    #[inline]
    pub async fn text(self) -> Result<String, Error> {
        self.0.text().await.map_err(MultipartError::read_error)
    }

    /// Stream a chunk of the field data.
    ///
    /// When the field data has been exhausted, this will return [`None`].
    ///
    ///> See also [`multer::Field::chunk`]
    #[inline]
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, Error> {
        self.0.chunk().await.map_err(MultipartError::read_error)
    }

    /// Asynchronously writes a multipart field as a file stream to disk with a name taken from `CONTENT_DISPOSITION` header
    ///
    /// # Example
    /// ```no_run
    /// use volga::{HttpResult, Multipart, ok};
    ///
    /// async fn handle(mut files: Multipart) -> HttpResult {
    ///     while let Some(field) = files.next_field().await? {
    ///         field.save("path/to/folder").await?;
    ///     }
    ///     ok!("File saved!")
    /// }
    /// ```
    #[inline]
    pub async fn save(self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let file_name = self.try_get_file_name()?;
        let file_path = path.as_ref().join(file_name);
        self.save_as(file_path).await
    }

    /// Asynchronously writes a multipart field as a file stream to disk with a provided name in `file_path`
    ///
    /// # Example
    /// ```no_run
    /// use volga::{HttpResult, Multipart, ok};
    /// use std::path::Path;
    ///
    /// async fn handle(mut files: Multipart) -> HttpResult {
    ///     let path = Path::new("path/to/folder");
    ///     let mut counter = 0;
    ///     while let Some(field) = files.next_field().await? {
    ///         field.save_as(path.join(format!("file_{counter}.dat"))).await?;
    ///         counter += 1;
    ///     }
    ///     ok!("File saved!")
    /// }
    /// ```
    #[inline]
    pub async fn save_as(mut self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let file = tokio::fs::File::create(path).await?;
        let mut writer = BufWriter::new(file);
        while let Some(ref chunk) = self.chunk().await? {
            writer.write_all(chunk).await?
        }
        writer.flush().await
    }
}

impl Deref for Field {
    type Target = multer::Field<'static>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Field {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

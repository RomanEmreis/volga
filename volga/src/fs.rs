//! File System tools and abstractions

use mime::Mime;
use std::path::Path;

#[cfg(feature = "static-files")]
pub mod static_files;

/// Returns a file MIME type or `application/octet-stream` if unable to determine
#[inline]
pub fn get_mime_or_octet_stream<P: AsRef<Path>>(path: P) -> Mime {
    mime_guess::from_path(&path)
        .first_or_octet_stream()
}
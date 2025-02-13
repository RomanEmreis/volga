use mime::Mime;
use std::path::Path;

#[cfg(feature = "static-files")]
pub mod static_files;

#[inline]
pub fn get_mime_or_octet_stream<P: AsRef<Path>>(path: P) -> Mime {
    mime_guess::from_path(&path)
        .first_or_octet_stream()
}
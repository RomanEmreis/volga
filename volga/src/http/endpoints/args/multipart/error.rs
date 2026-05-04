//! Multipart-specific error helpers.

use crate::error::Error;

pub(super) struct MultipartError;

impl MultipartError {
    #[inline]
    pub(super) fn invalid_boundary() -> Error {
        Error::client_error("Multipart error: invalid boundary")
    }

    #[inline]
    pub(super) fn missing_file_name() -> Error {
        Error::client_error("Multipart error: file name is missing")
    }

    #[inline]
    pub(super) fn read_error(error: multer::Error) -> Error {
        Error::client_error(format!("Multipart error: {error}"))
    }
}
